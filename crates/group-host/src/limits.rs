//! How much one caller may ask of the host, and how often: the reference
//! PDS's limits, and a few of the host's own where it asks other servers
//! or keeps what anyone sends. Each limit is a fixed window of points for
//! each key, begun at its first request: a client's address, a group, or
//! someone signed in. Past them, a request is refused as
//! `RateLimitExceeded`, a 429 saying when to try again. The windows are
//! kept in memory, as only one host runs over its data, and `RATE_LIMITS`
//! turns them off.
//!
//! The client is the address a request came from, unless that is this
//! machine or its private network, as a proxy in front is: then the
//! address `X-Forwarded-For` names, read from the right past each such
//! address. An IPv6 client is counted with the rest of its /64.

use crate::Host;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};

pub const MINUTE: i64 = 60 * 1000;
pub const HOUR: i64 = 60 * MINUTE;
pub const DAY: i64 = 24 * HOUR;

/// How many points a key may spend in a window of `window` milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    /// What it limits, its windows kept apart from every other's.
    pub name: &'static str,
    pub points: u32,
    pub window: i64,
}

/// A [`Limit`], as a constant: a macro, so that nothing runs to make one.
macro_rules! limit {
    ($name:literal, $points:expr, $window:expr) => {
        Limit {
            name: $name,
            points: $points,
            window: $window,
        }
    };
}

/// Every request from one address, but those [`by_path`] spares.
pub const BY_ADDRESS: Limit = limit!("address", 3000, 5 * MINUTE);
/// A whole repo, read from one address, as a relay reads them.
pub const SYNC_REPO: Limit = limit!("sync-repo", 6000, 5 * MINUTE);
/// Groups made from one address, each an identity in the directory.
pub const NEW_GROUPS: Limit = limit!("new-groups", 100, 5 * MINUTE);
/// An app's pushed request, which may fetch its metadata, from one
/// address.
pub const PUSHED: Limit = limit!("pushed", 300, 5 * MINUTE);
/// MCP clients registering from one address, each one kept a day.
pub const REGISTER: Limit = limit!("register", 30, 5 * MINUTE);
/// Signing in to the console, or to move an account here, from one
/// address: each asks the person's own server.
pub const SIGN_IN: Limit = limit!("sign-in", 30, 5 * MINUTE);
/// Searches for a handle being typed, from one address: each asks the
/// search service.
pub const SEARCH: Limit = limit!("search", 600, 5 * MINUTE);
/// Signing in with an app password, by the handle and address.
pub const SESSIONS: [Limit; 2] = [
    limit!("session", 30, 5 * MINUTE),
    limit!("session-day", 300, DAY),
];
/// What a group's tokens write, in points (see [`write_points`]).
pub const WRITES_HOUR: Limit = limit!("writes", 5000, HOUR);
pub const WRITES_DAY: Limit = limit!("writes-day", 35000, DAY);
pub const WRITES: [Limit; 2] = [WRITES_HOUR, WRITES_DAY];
/// Blobs a group's tokens upload.
pub const BLOBS: Limit = limit!("blobs", 1000, DAY);
/// Changes to a group's handle, each an operation in the directory.
pub const HANDLES: [Limit; 2] = [
    limit!("handle", 10, 5 * MINUTE),
    limit!("handle-day", 50, DAY),
];
/// Reports someone files, into groups' queues.
pub const REPORTS: [Limit; 2] = [
    limit!("reports", 10, 5 * MINUTE),
    limit!("reports-day", 100, DAY),
];
/// Requests to join someone makes.
pub const JOINS: [Limit; 2] = [
    limit!("joins", 10, 5 * MINUTE),
    limit!("joins-day", 100, DAY),
];

/// The headers a refusal says when to try again in, for a browser app to
/// read.
pub const EXPOSED: &str =
    "retry-after, ratelimit-limit, ratelimit-remaining, ratelimit-reset, ratelimit-policy";

/// A refusal: the limit spent, and when its window ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spent {
    /// The limit's points.
    pub limit: u32,
    /// Its window, in seconds.
    pub window: i64,
    /// When the window ends, in seconds since the epoch.
    pub resets: i64,
    /// Seconds until then, at least one.
    pub retry_after: i64,
}

struct Window {
    ends: i64,
    used: u32,
}

#[derive(Default)]
struct Windows {
    open: HashMap<(&'static str, String), Window>,
    /// When those ended are next let go.
    sweep_at: i64,
}

/// The windows of every limit and key.
pub struct Limiter {
    on: bool,
    windows: Mutex<Windows>,
}

impl Limiter {
    /// Limits that count, or, `on` false, that count nothing.
    pub fn new(on: bool) -> Limiter {
        Limiter {
            on,
            windows: Mutex::default(),
        }
    }

    /// Spends `points` of `limit` for `key` at `now`, in milliseconds:
    /// refused, spending nothing, when the window has fewer left.
    pub fn spend(&self, limit: &Limit, key: &str, points: u32, now: i64) -> Result<(), Spent> {
        if !self.on {
            return Ok(());
        }
        let mut held = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        if now >= held.sweep_at {
            held.open.retain(|_, w| w.ends > now);
            held.sweep_at = now + MINUTE;
        }
        let fresh = || Window {
            ends: now + limit.window,
            used: 0,
        };
        let window = held
            .open
            .entry((limit.name, key.into()))
            .or_insert_with(fresh);
        if window.ends <= now {
            *window = fresh();
        }
        if window.used + points > limit.points {
            return Err(Spent {
                limit: limit.points,
                window: limit.window / 1000,
                resets: window.ends / 1000,
                retry_after: (window.ends - now + 999) / 1000,
            });
        }
        window.used += points;
        Ok(())
    }
}

impl Host {
    /// Spends `points` of each of `limits` for `key`, now.
    pub fn spend(&self, limits: &[Limit], key: &str, points: u32) -> Result<(), XrpcError> {
        let now = now_millis();
        for limit in limits {
            self.limits.spend(limit, key, points, now)?;
        }
        Ok(())
    }

    /// Spends what `writes` cost of `group`'s writing.
    pub(crate) fn spend_writes(
        &self,
        group: &str,
        writes: &[crate::repo::Write],
    ) -> Result<(), XrpcError> {
        self.spend(&WRITES, group, write_points(writes))
    }
}

/// What writes cost, as the reference PDS counts them: a create three
/// points, an update two, a delete one.
pub fn write_points(writes: &[crate::repo::Write]) -> u32 {
    use crate::repo::Write;
    let each = |w: &Write| match w {
        Write::Create { .. } => 3,
        Write::Update { .. } => 2,
        Write::Delete { .. } => 1,
    };
    writes.iter().map(each).sum()
}

/// Whether `ip` is this machine or its private network, where a proxy in
/// front of the host is.
fn proxy(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local(),
    }
}

/// Who a request from `peer` is from, as `forwarded`
/// (`X-Forwarded-For`) names them when `peer` is a proxy here.
pub fn client(peer: IpAddr, forwarded: Option<&str>) -> IpAddr {
    let mut from = peer.to_canonical();
    let named = forwarded.unwrap_or_default().rsplit(',');
    for next in named.filter_map(|a| a.trim().parse::<IpAddr>().ok()) {
        if !proxy(from) {
            break;
        }
        from = next.to_canonical();
    }
    from
}

/// What a client is counted by: its address, or for IPv6, its /64.
pub fn key(ip: IpAddr) -> String {
    match ip.to_canonical() {
        IpAddr::V6(v6) => {
            let [a, b, c, d, ..] = v6.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
        v4 => v4.to_string(),
    }
}

/// The client a request is from, as [`by_address`] found it.
#[derive(Debug, Clone, Copy)]
pub struct Client(pub IpAddr);

/// The limits a request to `path` spends from its client's address.
fn by_path(path: &str) -> &'static [Limit] {
    match path {
        // Asked by the machine's own checks and the proxy, and a relay's
        // stream, open for as long as it reads.
        "/xrpc/_health" | "/tls/ask" | "/xrpc/com.atproto.sync.subscribeRepos" => &[],
        "/xrpc/com.atproto.sync.getRepo" => &[SYNC_REPO],
        "/xrpc/fyi.opensocial.internal.createGroup"
        | "/xrpc/fyi.opensocial.internal.provisionGroup" => &[BY_ADDRESS, NEW_GROUPS],
        "/oauth/par" => &[BY_ADDRESS, PUSHED],
        "/mcp/register" => &[BY_ADDRESS, REGISTER],
        "/console/login" | "/console/move/login" => &[BY_ADDRESS, SIGN_IN],
        "/xrpc/fyi.opensocial.internal.searchActors" => &[BY_ADDRESS, SEARCH],
        _ => &[BY_ADDRESS],
    }
}

/// Spends what a request costs its client's address, and notes the
/// client for the methods that count by it too. A request with no
/// address, made within the host, spends nothing.
pub async fn by_address(State(host): State<Arc<Host>>, mut req: Request, next: Next) -> Response {
    let peer = req.extensions().get::<ConnectInfo<SocketAddr>>();
    let peer = peer.map(|c| c.0.ip());
    if let Some(peer) = peer {
        let forwarded = req.headers().get("x-forwarded-for");
        let from = client(peer, forwarded.and_then(|v| v.to_str().ok()));
        if let Err(refused) = host.spend(by_path(req.uri().path()), &key(from), 1) {
            return refused.into_response();
        }
        req.extensions_mut().insert(Client(from));
    }
    next.run(req).await
}
