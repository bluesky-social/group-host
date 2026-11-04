//! Joining a group: at once for one whose profile says `open`, or for one
//! holding its invite; by a request someone holding `admit` decides on
//! for one that says `approval`; not without an invite for one that says
//! `invite`.
//!
//! A request waits in the host, not in the group's records: who asks is
//! no member, and the members space is for members. Decided, it goes: a
//! request turned down looks the same as one never made, so its asker may
//! ask again and the group need not say.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::group::meta_space;
use crate::group_api::{group_of, signed_in};
use crate::group_members::{Members, acting};
use crate::rbac::{NS, listed};
use crate::store::JoinRequest;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

fn no_request() -> XrpcError {
    bad("JoinRequestNotFound", "no request from them is waiting")
}

/// A request as a view shows it.
pub fn request_view(r: &JoinRequest) -> Value {
    let mut view = json!({"did": r.did, "createdAt": iso(r.created_at)});
    if let Some(message) = &r.message {
        view["message"] = json!(message);
    }
    view
}

/// Asks to join: admitted at once to a group open to all, queued for one
/// that approves its members, refused without an invite for one that
/// invites them.
pub async fn request_join(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.requestJoin")).await?;
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    group_of(&host, group)?;
    host.active(group)?;
    let members = Members::of(&host, group)?;
    if members.membership(&me)?.is_some() {
        return Err(bad("AlreadyMember", "you are a member already"));
    }
    // An invite is pre-approval: redeemed, whatever the policy.
    if host.redeem(group, &me).await? {
        let roles = members.default_roles()?;
        members.put_membership(&me, &roles).await?;
        host.store.decide(group, &me).map_err(XrpcError::internal)?;
        return Ok(Json(json!({"status": "admitted"})));
    }
    let profile = host
        .group_record(group, &meta_space(group), &format!("{NS}.profile"), "self")
        .map_err(XrpcError::internal)?
        .unwrap_or_default();
    match profile["joinPolicy"].as_str() {
        Some("open") => {
            let roles = members.default_roles()?;
            members.put_membership(&me, &roles).await?;
            // A request they made before the group opened waits no more.
            host.store.decide(group, &me).map_err(XrpcError::internal)?;
            Ok(Json(json!({"status": "admitted"})))
        }
        Some("invite") => Err(bad("InviteRequired", "this group joins by invite")),
        _ => {
            let message = body["message"].as_str();
            // As the lexicon has it: 300 graphemes, 3000 bytes, at most.
            if message.is_some_and(|m| m.len() > 3000 || m.chars().count() > 300) {
                return Err(invalid("a message is 300 characters at most"));
            }
            host.store
                .ask(group, &me, message, now_millis())
                .map_err(XrpcError::internal)?;
            Ok(Json(json!({"status": "pending"})))
        }
    }
}

/// The caller's own request to join, while it waits.
pub async fn get_join_request(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.getJoinRequest")).await?;
    let group = param(&p, "group")?;
    group_of(&host, group)?;
    let asked = host.store.asked(group, &me).map_err(XrpcError::internal)?;
    Ok(Json(match asked {
        Some(r) => json!({ "request": request_view(&r) }),
        None => json!({}),
    }))
}

/// The requests waiting to join, newest first, for one holding `admit`.
pub async fn list_join_requests(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    let body = json!({ "group": group });
    let (members, _, _) = acting(&host, &headers, &body, "listJoinRequests", Some("admit")).await?;
    let limit = page(&p, 50, 100)?;
    let before = match p.get("cursor") {
        None => None,
        Some(c) => Some(
            c.parse::<i64>()
                .map_err(|_| invalid("a cursor this host gave"))?,
        ),
    };
    let waiting = host
        .store
        .asking(&members.group, before, limit)
        .map_err(XrpcError::internal)?;
    let cursor = waiting
        .last()
        .filter(|_| waiting.len() == limit as usize)
        .map(|r| r.seq.to_string());
    let requests: Vec<Value> = waiting.iter().map(request_view).collect();
    Ok(Json(json!({"requests": requests, "cursor": cursor})))
}

/// Admits one who asked, with the roles named or the group's defaults,
/// each one the caller may give: admitting gives roles, so it is bound as
/// giving them is.
pub async fn admit_member(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, _, mine) = acting(&host, &headers, &body, "admitMember", Some("admit")).await?;
    let did = body["did"]
        .as_str()
        .ok_or_else(|| invalid("did is required"))?;
    let asked = host
        .store
        .asked(&members.group, did)
        .map_err(XrpcError::internal)?;
    if asked.is_none() {
        return Err(no_request());
    }
    // A member by now, by another way in: their request is stale, and
    // their roles are not this method's to replace.
    if members.membership(did)?.is_some() {
        host.store
            .decide(&members.group, did)
            .map_err(XrpcError::internal)?;
        return Err(bad("AlreadyMember", "they are a member already"));
    }
    let roles = match body.get("roles") {
        Some(_) => listed(&body, "roles", "role"),
        None => members.default_roles()?,
    };
    if roles.is_empty() {
        return Err(invalid("roles names the roles to give"));
    }
    // The roles an admitter may give are those their `admit` grants may.
    let assignable = members.assignable(&mine, "admit");
    let blocked: Vec<&String> = roles.iter().filter(|r| !assignable.contains(*r)).collect();
    if !blocked.is_empty() {
        return Err(bad(
            "NotAssignable",
            format!("not yours to give: {blocked:?}"),
        ));
    }
    members.put_membership(did, &roles).await?;
    host.store
        .decide(&members.group, did)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// Turns down one who asked, for one holding `admit`. They may ask again.
pub async fn deny_join_request(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, _, _) = acting(&host, &headers, &body, "denyJoinRequest", Some("admit")).await?;
    let did = body["member"]
        .as_str()
        .ok_or_else(|| invalid("member is required"))?;
    let decided = host.store.decide(&members.group, did);
    if !decided.map_err(XrpcError::internal)? {
        return Err(no_request());
    }
    Ok(Json(json!({})))
}

/// Withdraws the caller's own request to join.
pub async fn cancel_join_request(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.cancelJoinRequest")).await?;
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    group_of(&host, group)?;
    if !host.store.decide(group, &me).map_err(XrpcError::internal)? {
        return Err(no_request());
    }
    Ok(Json(json!({})))
}
