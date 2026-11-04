//! Switching a group off, back on, and deleting it, as `lifecycle.rs`
//! does to its account.
//!
//! While a group is on, who may do each is who holds `group.configure`;
//! while it is off nothing can read its memberships, so it is who held it
//! when it was switched off, kept then.
//!
//! As the prototype's `methods/lifecycle.ts`.

use crate::Host;
use crate::api::invalid;
use crate::group_api::{group_of, signed_in};
use crate::group_config::forbidden;
use crate::rbac::NS;
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: &str) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// The group a body names, and who asks for `lxm` on it.
async fn asked(
    host: &Host,
    headers: &HeaderMap,
    body: &Value,
    lxm: &str,
) -> Result<(String, String), XrpcError> {
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    group_of(host, group)?;
    Ok((group.to_string(), me))
}

/// Whether `me` may switch `group` back on or delete it: by its records
/// while it is on, by who held `group.configure` when it went off while
/// it is off.
fn steward(host: &Host, group: &str, me: &str) -> Result<(), XrpcError> {
    let may = match host.deactivation(group)? {
        Some(off) => off.stewards.iter().any(|s| s == me),
        None => host.may_configure(group, me)?,
    };
    if may { Ok(()) } else { Err(forbidden()) }
}

/// Switches a group off: nothing reads its spaces or writes as it until a
/// steward of now switches it back on.
pub async fn deactivate_group(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, me) = asked(&host, &headers, &body, "deactivateGroup").await?;
    if host.deactivation(&group)?.is_some() {
        return Err(bad(
            "AlreadyDeactivated",
            "the group is switched off already",
        ));
    }
    steward(&host, &group, &me)?;
    // Read before it goes dark: afterwards nothing can read them.
    let stewards = host.configurers(&group)?;
    host.deactivate_account(&group, &me, &stewards)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// Switches a group back on, for one who could switch it off.
pub async fn activate_group(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, me) = asked(&host, &headers, &body, "activateGroup").await?;
    if host.deactivation(&group)?.is_none() {
        return Err(bad("NotDeactivated", "the group is not switched off"));
    }
    steward(&host, &group, &me)?;
    host.activate_account(&group).map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// Deletes a group: its account, repos and spaces here, and what the host
/// keeps of them; the services syncing its spaces are told; its DID is
/// ended. What members wrote in its spaces is theirs, at their own PDSes.
pub async fn delete_group(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, me) = asked(&host, &headers, &body, "deleteGroup").await?;
    steward(&host, &group, &me)?;
    host.tell_spaces_deleted(&group)
        .map_err(XrpcError::internal)?;
    let tombstoned = host
        .delete_account(&group)
        .await
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({ "tombstoned": tombstoned })))
}
