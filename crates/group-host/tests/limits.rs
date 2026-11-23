//! How much one caller may ask, and how often: a window of points for
//! each limit and key, the client's address as a proxy here names it,
//! and the refusal, a 429 saying when to try again.

mod common;

use common::groups::*;
use common::members::*;
use common::spaces::*;
use common::{Running, start};
use group_host::limits::{self, Limit, Limiter, client, key};
use serde_json::json;
use std::net::IpAddr;

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn a_window_holds_its_points_then_refuses_until_it_ends() {
    let limiter = Limiter::new(true);
    let limit = Limit {
        name: "test",
        points: 5,
        window: 10_000,
    };
    limiter.spend(&limit, "a", 2, 1_000).unwrap();
    limiter.spend(&limit, "a", 3, 2_000).unwrap();
    // Spent, it says when the window, begun at the first, ends.
    let refused = limiter.spend(&limit, "a", 1, 2_500).unwrap_err();
    assert_eq!(refused.limit, 5);
    assert_eq!(refused.window, 10);
    assert_eq!(refused.resets, 11);
    assert_eq!(refused.retry_after, 9);
    // A refusal spends nothing, and another key has a window of its own.
    limiter.spend(&limit, "b", 5, 2_500).unwrap();
    // Once the window ends, a new one begins.
    limiter.spend(&limit, "a", 5, 11_000).unwrap();
    assert!(limiter.spend(&limit, "a", 1, 11_001).is_err());
    // More than a window holds is refused even in a new one.
    assert!(limiter.spend(&limit, "c", 6, 0).is_err());
    // A minute on, the windows that ended are let go, and those still open
    // are kept.
    limiter.spend(&limit, "d", 5, 60_000).unwrap();
    limiter.spend(&limit, "e", 1, 61_000).unwrap();
    assert!(limiter.spend(&limit, "d", 1, 61_500).is_err());
    limiter.spend(&limit, "a", 5, 61_500).unwrap();
    // Off, nothing is counted.
    let off = Limiter::new(false);
    for _ in 0..10 {
        off.spend(&limit, "a", 5, 0).unwrap();
    }
}

#[test]
fn the_client_is_the_peer_unless_a_proxy_here_forwards_it() {
    // From the open network, what it forwards is its own word.
    assert_eq!(
        client(ip("198.51.100.1"), Some("203.0.113.9")),
        ip("198.51.100.1")
    );
    // From this machine or the private network, the first address read
    // from the right that is not one too.
    assert_eq!(client(ip("127.0.0.1"), None), ip("127.0.0.1"));
    assert_eq!(
        client(ip("127.0.0.1"), Some("203.0.113.9")),
        ip("203.0.113.9")
    );
    assert_eq!(
        client(
            ip("172.18.0.3"),
            Some("198.51.100.4, 203.0.113.9, 10.0.0.2")
        ),
        ip("203.0.113.9")
    );
    assert_eq!(
        client(ip("fd00::1"), Some("garbage, 2001:db8::7")),
        ip("2001:db8::7")
    );
    // Nothing but proxies: the furthest of them.
    assert_eq!(
        client(ip("::ffff:127.0.0.1"), Some("10.0.0.9, 192.168.1.2")),
        ip("10.0.0.9")
    );
    assert_eq!(
        client(ip("fe80::1"), Some("169.254.0.1")),
        ip("169.254.0.1")
    );
    // An IPv6 address is counted with the rest of its /64, one network's.
    assert_eq!(key(ip("2001:db8:1:2:3:4:5:6")), "2001:db8:1:2::/64");
    assert_eq!(key(ip("::ffff:203.0.113.9")), "203.0.113.9");
}

async fn register(h: &Running, from: Option<&str>) -> reqwest::Response {
    let mut req = h
        .http
        .post(format!("{}/mcp/register", h.url))
        .json(&json!({"redirect_uris": ["http://127.0.0.1/back"]}));
    if let Some(from) = from {
        req = req.header("x-forwarded-for", from);
    }
    req.send().await.unwrap()
}

#[tokio::test]
async fn a_flood_from_one_address_is_refused() {
    let h = start(&[], false).await;
    for _ in 0..limits::REGISTER.points {
        assert_eq!(register(&h, None).await.status(), 201);
    }
    let refused = register(&h, None).await;
    assert_eq!(refused.status(), 429);
    let headers = refused.headers().clone();
    let said: serde_json::Value = refused.json().await.unwrap();
    assert_eq!(said["error"], "RateLimitExceeded");
    assert_eq!(headers["ratelimit-limit"], "30");
    assert_eq!(headers["ratelimit-remaining"], "0");
    assert_eq!(headers["ratelimit-policy"], "30;w=300");
    let retry: i64 = headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!((1..=300).contains(&retry), "{retry}");
    let reset: i64 = headers["ratelimit-reset"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(reset > now() as i64, "{reset}");
    // A browser app reads why.
    assert_eq!(headers["access-control-allow-origin"], "*");
    let exposed = headers["access-control-expose-headers"].to_str().unwrap();
    assert!(exposed.contains("retry-after"), "{exposed}");
    // Another client, as a proxy here names it, has its own.
    assert_eq!(register(&h, Some("203.0.113.9")).await.status(), 201);
    // The host's health is asked however often.
    let (status, _) = h.get_path("/xrpc/_health").await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn every_request_counts_toward_its_address_but_health_and_sync() {
    let h = start(&[], false).await;
    let all = limits::BY_ADDRESS.points;
    h.host
        .limits
        .spend(
            &limits::BY_ADDRESS,
            "127.0.0.1",
            all,
            group_host::time::now_millis(),
        )
        .unwrap();
    let (status, said) = h.get("com.atproto.server.describeServer", "").await;
    assert_eq!(
        (status, said["error"].as_str()),
        (429, Some("RateLimitExceeded"))
    );
    let (status, _) = h.get_path("/xrpc/_health").await;
    assert_eq!(status, 200);
    // A relay's crawl has a limit of its own.
    let (status, _) = h
        .get("com.atproto.sync.getRepo", "did=did:plc:nobody")
        .await;
    assert_ne!(status, 429);
}

#[tokio::test]
async fn off_refuses_nothing() {
    let h = start(&[("RATE_LIMITS", "off")], false).await;
    for _ in 0..=limits::REGISTER.points {
        assert_eq!(register(&h, None).await.status(), 201);
    }
}

#[tokio::test]
async fn a_groups_writes_are_counted_by_what_they_make() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let runs = Session::runs(&h, &alice, &group).await;
    let hour = limits::WRITES_HOUR.points;
    let now = group_host::time::now_millis();
    h.host
        .limits
        .spend(&limits::WRITES_HOUR, &group, hour - 2, now)
        .unwrap();
    // A create is three points, and two are left.
    let said = runs.create(&h, "role", "guide", role("Guide")).await;
    assert_eq!(said.error(), (429, "RateLimitExceeded"));
    assert!(said.headers.contains_key("retry-after"));
    // A put is two; then a delete, one, is more than is left.
    let said = runs.put(&h, "role", "guide", role("Guide")).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let said = runs.delete(&h, "role", "guide").await;
    assert_eq!(said.error(), (429, "RateLimitExceeded"));
}

#[tokio::test]
async fn asking_to_join_is_counted_by_who_asks() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let [five_minutes, _] = limits::JOINS;
    let now = group_host::time::now_millis();
    h.host
        .limits
        .spend(&five_minutes, &bob.did, five_minutes.points, now)
        .unwrap();
    let lxm = format!("{NS}.requestJoin");
    let body = json!({"group": group});
    let (status, said) = post(&h, &lxm, &body, &[auth(&h, &bob, &lxm)]).await;
    assert_eq!(
        (status, said["error"].as_str()),
        (429, Some("RateLimitExceeded"))
    );
    let (status, said) = post(&h, &lxm, &body, &[auth(&h, &carol, &lxm)]).await;
    assert_eq!(status, 200, "{said}");
}
