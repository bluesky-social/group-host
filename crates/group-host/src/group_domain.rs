//! A group at its own domain: the domain of its handle, once the group
//! takes one ([`crate::group_handle`]).
//!
//! A group's address is a name under the host's own domain it was made
//! at. The host keeps its own paths there, `/.well-known/` and `/xrpc/`,
//! and sends a visitor at `/` on to the group's page in the console.
//!
//! The handle proves the group controls the domain, not that the domain's
//! web traffic reaches this host: its A record may still name another
//! server. So the group moves there only when a steward switches it on,
//! and only if the domain answers as this host then. Once on, the domain
//! keeps the host's own paths as the address does, and the address and
//! `www.` under the domain redirect to it. Switched off, or the handle
//! changed, the group is at its address alone again.
//!
//! A proxy in front issues the domain's certificate as it is first asked
//! for, asking [`tls_ask`] first, so nobody can point DNS at the host and
//! spend its certificate limits.

use crate::Host;
use crate::group_api::{group_of, signed_in};
use crate::rbac::INTERNAL;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto_client::identity::LOOKUP;
use axum::Json;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures::future::OptionFuture;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: String) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// The group a body names, once its caller of `lxm` may configure it.
async fn configuring(
    host: &Host,
    headers: &HeaderMap,
    body: &Value,
    lxm: &str,
) -> Result<String, XrpcError> {
    let me = signed_in(host, headers, &format!("{INTERNAL}.{lxm}")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(host, group)?;
    if !host.may_configure(group, &me)? {
        return Err(crate::group_config::forbidden());
    }
    Ok(group.to_string())
}

impl Host {
    /// The domain of `group`'s handle, when it is one of its own and not
    /// its address here.
    fn own_domain(&self, group: &str) -> Result<Option<String>, XrpcError> {
        let account = self.store.account(group).map_err(XrpcError::internal)?;
        let address = self.store.address(group).map_err(XrpcError::internal)?;
        Ok(account
            .map(|a| a.handle)
            .filter(|h| Some(h) != address.as_ref()))
    }

    /// Whether `domain` answers as this host: its `describeServer` naming
    /// this host's DID. Asked at `DOMAIN_CHECK_AT`, as it is set, with the
    /// domain as its `Host`; or else at the domain itself, a group's to
    /// name, over https as the host holds any host others' data names.
    async fn reaches(&self, domain: &str) -> bool {
        let path = "/xrpc/com.atproto.server.describeServer";
        let asked = match &self.config.domain_check_at {
            Some(at) => Some(self.http.get(format!("{at}{path}"))),
            None => self.guarded.get(&format!("https://{domain}{path}")).ok(),
        };
        let asked = asked.map(|a| a.header(header::HOST, domain).timeout(LOOKUP).send());
        let answered = OptionFuture::from(asked).await.and_then(Result::ok);
        let Some(answer) = answered else {
            return false;
        };
        let said: Value = answer.json().await.unwrap_or_default();
        said["did"] == self.config.service_did.as_str()
    }

    /// Where a request to `name` finds a group: the group whose address or
    /// own domain it is, or the domain it is served at instead (from its
    /// address, or `www.` under that domain); nothing if `name` is the
    /// host's own or no group's.
    fn group_at(&self, name: &str) -> Option<At> {
        let name = name
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let own = self
            .config
            .public_url
            .split("://")
            .nth(1)
            .unwrap_or_default();
        if name.is_empty() || name == own.split(':').next().unwrap_or_default() {
            return None;
        }
        let here = |did: String| {
            let account = self.store.account(&did).ok().flatten()?;
            Some(At::Here(account.did, account.handle))
        };
        if let Some(did) = self.store.addressed(&name).ok().flatten() {
            return match self.store.domain(&did).ok().flatten() {
                Some(domain) => Some(At::There(domain)),
                None => here(did),
            };
        }
        if let Some(did) = self.store.domained(&name).ok().flatten() {
            return here(did);
        }
        let apex = name.strip_prefix("www.")?;
        self.store.domained(apex).ok().flatten()?;
        Some(At::There(apex.to_string()))
    }
}

/// Where a request to a name finds a group.
enum At {
    /// Here: the group, and its handle.
    Here(String, String),
    /// At this domain instead, the group's own.
    There(String),
}

fn plain(status: StatusCode, text: &'static str) -> Response {
    let kind = [(header::CONTENT_TYPE, "text/plain; charset=utf-8")];
    (status, kind, text).into_response()
}

fn moved(to: &str, status: StatusCode) -> Response {
    let to = HeaderValue::from_str(to).expect("a path or address the host made");
    (status, [(header::LOCATION, to)]).into_response()
}

/// Answers a request to a group's address or own domain: the host's own
/// paths under `/.well-known/` and `/xrpc/` pass on, a request where the
/// group is not redirects to where it is, and its front page redirects
/// to its page in the console; nothing else is there. Passes on any
/// request to another name.
pub async fn serve(State(host): State<Arc<Host>>, req: Request, next: Next) -> Response {
    let named = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok());
    let group = named.and_then(|n| host.group_at(n));
    let path = req.uri().path();
    let ours = path.starts_with("/.well-known/") || path.starts_with("/xrpc/");
    let (Some(at), false) = (group, ours) else {
        return next.run(req).await;
    };
    let (did, handle) = match at {
        At::Here(did, handle) => (did, handle),
        At::There(domain) => {
            let asked = req.uri().path_and_query().map_or("/", |p| p.as_str());
            let to = format!("https://{domain}{asked}");
            return moved(&to, StatusCode::MOVED_PERMANENTLY);
        }
    };
    let read = [Method::GET, Method::HEAD].contains(req.method());
    if read && path == "/" && host.active(&did).is_ok() {
        let page = format!("{}/g/{handle}", host.config.console_url);
        return moved(&page, StatusCode::FOUND);
    }
    plain(StatusCode::NOT_FOUND, "nothing here")
}

/// Serves the group at its handle's own domain, once that domain reaches
/// this host.
pub async fn put_group_domain(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = configuring(&host, &headers, &body, "putGroupDomain").await?;
    host.active(&group)?;
    let Some(domain) = host.own_domain(&group)? else {
        let why = "the group's handle is its address: take a handle on a domain of its own first";
        return Err(bad("NoOwnDomain", why.into()));
    };
    if !host.reaches(&domain).await {
        let why = format!(
            "{domain} doesn't reach this host yet: point its A and AAAA records at this host's addresses, then try again"
        );
        return Err(bad("NotReaching", why));
    }
    let set = host.store.set_domain(&group, Some(&domain), now_millis());
    set.map_err(XrpcError::internal)?;
    Ok(Json(json!({ "domain": domain })))
}

/// Serves the group at its address alone again.
pub async fn delete_group_domain(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = configuring(&host, &headers, &body, "deleteGroupDomain").await?;
    let set = host.store.set_domain(&group, None, now_millis());
    set.map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// Whether a proxy in front may get a certificate for `domain`: the own
/// domain of a group that is on, or `www.` under it, whether or not it is
/// switched on yet, so that the host can check the domain reaches it.
pub async fn tls_ask(
    State(host): State<Arc<Host>>,
    Query(p): Query<HashMap<String, String>>,
) -> StatusCode {
    let asked = p.get("domain").map(|d| d.to_ascii_lowercase());
    let asked = asked.unwrap_or_default();
    let domain = asked.strip_prefix("www.").unwrap_or(&asked);
    let account = host.store.account(domain).ok().flatten();
    let group = account.filter(|a| a.handle == domain).map(|a| a.did);
    let ours = group.filter(|g| {
        host.own_domain(g)
            .ok()
            .flatten()
            .is_some_and(|d| d == domain)
    });
    match ours.filter(|g| host.active(g).is_ok()) {
        Some(_) => StatusCode::OK,
        None => StatusCode::NOT_FOUND,
    }
}
