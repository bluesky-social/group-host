//! A group's site at its own domain: the domain of its handle, once the
//! group takes one ([`crate::group_handle`]).
//!
//! The handle proves the group controls the domain, not that the domain's
//! web traffic reaches this host: its A record may still name an old site.
//! So the site moves there only when a steward switches it on, and only if
//! the domain answers as this host then. Once on, the domain serves the
//! site, `www.` under it and the address redirect to it, and the address
//! keeps the host's own paths. Switched off, or the handle changed, the
//! site is at its address alone again.
//!
//! A proxy in front issues the domain's certificate as it is first asked
//! for, asking [`tls_ask`] first, so nobody can point DNS at the host and
//! spend its certificate limits.

use crate::Host;
use crate::identity::LOOKUP;
use crate::sites::configuring;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: String) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
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
    /// this host's DID. Asked at the domain over https, or at
    /// `SITE_CHECK_AT` with the domain as its `Host`.
    async fn reaches(&self, domain: &str) -> bool {
        let base = match &self.config.site_check_at {
            Some(at) => at.clone(),
            None => format!("https://{domain}"),
        };
        let url = format!("{base}/xrpc/com.atproto.server.describeServer");
        let asked = self
            .http
            .get(url)
            .header(header::HOST, domain)
            .timeout(LOOKUP);
        let Ok(answer) = asked.send().await else {
            return false;
        };
        let said: Value = answer.json().await.unwrap_or_default();
        said["did"] == self.config.service_did.as_str()
    }
}

/// Serves the group's site at its handle's own domain, once that domain
/// reaches this host.
pub async fn put_site_domain(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = configuring(&host, &headers, &body, "putSiteDomain").await?;
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
    let set = host
        .store
        .set_site_domain(&group, Some(&domain), now_millis());
    set.map_err(XrpcError::internal)?;
    Ok(Json(json!({ "domain": domain })))
}

/// Serves the group's site at its address alone again.
pub async fn delete_site_domain(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = configuring(&host, &headers, &body, "deleteSiteDomain").await?;
    let set = host.store.set_site_domain(&group, None, now_millis());
    set.map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// Whether a proxy in front may get a certificate for `domain`: the own
/// domain of a group that is on, or `www.` under it, whether or not its
/// site is there yet, so that the host can check the domain reaches it.
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
