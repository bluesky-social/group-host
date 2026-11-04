//! A group's handle on a domain of its own.
//!
//! A group starts with a handle under `HANDLE_DOMAIN`, which is also its
//! address, kept for it whatever handle it takes. It may take a handle on a domain of
//! its own instead, once the domain names it (by its `_atproto` DNS record
//! or its `/.well-known/atproto-did`, as any handle is resolved): the
//! handle is then its name everywhere, and goes with it to another host.
//! The address stays. Under the host's domain a group may take only its
//! own address back, so no group takes another's.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::group_api::{group_of, signed_in};
use crate::group_config::forbidden;
use crate::rbac::NS;
use crate::repo::identity;
use crate::xrpc::XrpcError;
use atproto::plc::Signed;
use atproto::syntax;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// The group a caller of `lxm` may configure, and its address.
async fn configurable(
    host: &Host,
    headers: &HeaderMap,
    group: &str,
    lxm: &str,
) -> Result<String, XrpcError> {
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    group_of(host, group)?;
    if !host.may_configure(group, &me)? {
        return Err(forbidden());
    }
    let address = host.store.address(group).map_err(XrpcError::internal)?;
    Ok(address.expect("a group has an address"))
}

/// The handle a group could take, as the host reads it: refused when it
/// never could.
fn wanted(host: &Host, group: &str, address: &str, asked: &str) -> Result<String, XrpcError> {
    let handle = asked.trim().trim_start_matches('@').to_ascii_lowercase();
    if !syntax::is_handle(&handle) {
        return Err(bad("InvalidHandle", format!("{handle} is no handle")));
    }
    let domain = host.config.handle_domain.trim_start_matches('.');
    let under = handle == domain || handle.ends_with(&host.config.handle_domain);
    if under && handle != address {
        return Err(bad(
            "UnsupportedDomain",
            format!("under {domain}, a group may have only its own address, {address}"),
        ));
    }
    let holder = host.store.account(&handle).map_err(XrpcError::internal)?;
    if holder.is_some_and(|a| a.did != group && a.handle == handle) {
        return Err(bad(
            "HandleNotAvailable",
            "another account here has that handle",
        ));
    }
    Ok(handle)
}

/// The DID `handle` names now: the group, for its own address.
async fn named(host: &Host, group: &str, address: &str, handle: &str) -> Option<String> {
    if handle == address {
        return Some(group.to_string());
    }
    let resolvers = &host.config.handle_resolvers;
    host.identity.resolve_handle(handle, resolvers).await
}

/// Whether a domain is ready to be the group's handle. Changes nothing.
pub async fn check_group_handle(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    let address = configurable(&host, &headers, group, "checkGroupHandle").await?;
    let handle = wanted(&host, group, &address, param(&p, "handle")?)?;
    let did = named(&host, group, &address, &handle).await;
    let mut out = json!({"handle": handle, "ready": did.as_deref() == Some(group)});
    if let Some(did) = did {
        out["did"] = json!(did);
    }
    Ok(Json(out))
}

/// Gives the group a handle on a domain that names it, or its own address
/// back: its `did:plc` first, then the host, then the relays.
pub async fn update_group_handle(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    let asked = body["handle"]
        .as_str()
        .ok_or_else(|| invalid("handle is required"))?;
    let address = configurable(&host, &headers, group, "updateGroupHandle").await?;
    let handle = wanted(&host, group, &address, asked)?;
    let current = host.store.account(group).map_err(XrpcError::internal)?;
    if current.is_some_and(|a| a.handle == handle) {
        return Ok(Json(json!({ "handle": handle })));
    }
    if named(&host, group, &address, &handle).await.as_deref() != Some(group) {
        return Err(bad(
            "NotPointing",
            format!(
                "{handle} does not name this group yet: its _atproto record should say did={group}"
            ),
        ));
    }
    // The directory first: until it names the handle, nobody takes it for
    // the group's.
    let turn = host.plc.turn().await;
    let last = host.plc.last_op(group).await.map_err(XrpcError::internal)?;
    let Signed::Update { op, .. } = &last else {
        return Err(bad("NotController", "the group's DID is ended"));
    };
    let ours = host.secrets.rotation.public().did_key();
    if !op.rotation_keys.contains(&ours) {
        return Err(bad(
            "NotController",
            "this host no longer holds a rotation key for the group",
        ));
    }
    let mut next = op.clone();
    next.also_known_as = vec![format!("at://{handle}")];
    let signed = next.sign(Some(last.cid()), &host.secrets.rotation);
    host.plc
        .publish(group, &signed)
        .await
        .map_err(crate::plc::not_published)?;
    drop(turn);
    // Taken here meanwhile, the database's unique handle refuses it.
    let set = host
        .store
        .set_handle(group, &handle, &[identity(group, &handle)]);
    set.map_err(XrpcError::internal)?;
    host.announce();
    Ok(Json(json!({ "handle": handle })))
}
