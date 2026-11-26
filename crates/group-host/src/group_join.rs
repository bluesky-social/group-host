//! Joining a group: at once for one whose config says `open`, or for one
//! holding its invite; by a request someone holding `admit` decides on
//! for one that says `approval`; not without an invite for one that says
//! `invite`. A request is granted by writing the asker's membership as
//! the group (`member_writes.rs`), and turned down by `denyJoinRequest`.
//! Asking again, a member is answered with their membership, and one
//! whose request waits with that request.
//!
//! A request waits in the host, not in the group's records: who asks is
//! no member, and the members space is for members. Decided, it goes: a
//! request turned down looks the same as one never made, so its asker may
//! ask again and the group need not say.

use crate::Host;
use crate::api::{Params, invalid_cursor, page, param};
use crate::group::meta_space;
use crate::group_api::{doing, group_of, signed_in};
use crate::group_invites::invite_uri;
use crate::group_members::Members;
use crate::oauth_resource::Refusal;
use crate::rbac::{INTERNAL, NS};
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

/// A request as the lexicons' `joinRequest` has it.
pub fn request_view(r: &JoinRequest) -> Value {
    let mut view = json!({"member": r.did, "createdAt": iso(r.created_at)});
    if let Some(message) = &r.message {
        view["message"] = json!(message);
    }
    view
}

/// `did` admitted to `group` with its default roles, a request they made
/// before waiting no more: the answer, naming their membership.
async fn admitted(members: &Members<'_>, did: &str) -> Answer {
    let roles = members.default_roles()?;
    let membership = members.put_membership(did, &roles).await?;
    let decided = members.host.store.decide(&members.group, did);
    decided.map_err(XrpcError::internal)?;
    Ok(Json(
        json!({"status": "admitted", "membership": membership}),
    ))
}

/// Asks to join: admitted at once to a group open to all, queued for one
/// that approves its members, refused without an invite for one that
/// invites them. An invite admits them whatever the policy. A member is
/// answered with their membership; one whose request waits, in a group
/// that is not open, with that request.
pub async fn request_join(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.requestJoin")).await?;
    host.spend(&crate::limits::JOINS, &me, 1)?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    host.active(group)?;
    let members = Members::of(&host, group)?;
    if let Some(membership) = members.membership_ref(&me)? {
        return Ok(Json(
            json!({"status": "admitted", "membership": membership}),
        ));
    }
    let waiting = host.waiting_invite(group, &me).await?;
    if let Some(named) = body["invite"].as_str()
        && waiting.as_ref().is_none_or(|w| invite_uri(w) != named)
    {
        return Err(bad(
            "InvalidInvite",
            "that invite is not one from this group to you, waiting",
        ));
    }
    // An invite is pre-approval: redeemed, whatever the policy. It is
    // taken back once they are a member, so a membership not written
    // leaves it waiting.
    if let Some(invite) = waiting {
        let answer = admitted(&members, &me).await?;
        host.take_back(&invite).await?;
        return Ok(answer);
    }
    let config = host
        .group_record(group, &meta_space(group), &format!("{NS}.config"), "self")
        .map_err(XrpcError::internal)?
        .unwrap_or_default();
    if config["joinPolicy"] == "open" {
        return admitted(&members, &me).await;
    }
    let pending = |asked: Option<JoinRequest>| {
        let request = request_view(&asked.expect("a request waiting"));
        Json(json!({"status": "pending", "request": request}))
    };
    let asked = host.store.asked(group, &me).map_err(XrpcError::internal)?;
    if asked.is_some() {
        return Ok(pending(asked));
    }
    if config["joinPolicy"] == "invite" {
        return Err(bad("JoinClosed", "this group joins by invite"));
    }
    let message = body["message"].as_str();
    host.store
        .ask(group, &me, message, now_millis())
        .map_err(XrpcError::internal)?;
    let asked = host.store.asked(group, &me).map_err(XrpcError::internal)?;
    Ok(pending(asked))
}

/// The caller's own request to join, while it waits.
pub async fn get_join_request(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.getJoinRequest")).await?;
    let group = param(&p, "group")?;
    group_of(&host, group)?;
    let asked = host.store.asked(group, &me).map_err(XrpcError::internal)?;
    Ok(Json(match asked {
        Some(r) => json!({ "request": request_view(&r) }),
        None => json!({}),
    }))
}

/// The requests waiting to join, newest first, for one holding `admit`,
/// or an app signed in as the group with its `rpc:` scope.
pub async fn list_join_requests(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Json<Value>, Refusal> {
    let lxm = format!("{NS}.listJoinRequests");
    let asked = p.get("group").map(String::as_str);
    let (group, _) = doing(&host, &headers, "GET", &lxm, asked, "admit").await?;
    let limit = page(&p, 50, 100)?;
    let before = match p.get("cursor") {
        None => None,
        Some(c) => Some(c.parse::<i64>().map_err(|_| invalid_cursor())?),
    };
    let waiting = host
        .store
        .asking(&group, before, limit)
        .map_err(XrpcError::internal)?;
    let cursor = waiting
        .last()
        .filter(|_| waiting.len() == limit as usize)
        .map(|r| r.seq.to_string());
    let requests: Vec<Value> = waiting.iter().map(request_view).collect();
    Ok(Json(json!({"requests": requests, "cursor": cursor})))
}

/// Turns down one who asked, for one holding `admit`, or an app signed in
/// as the group with its `rpc:` scope. They may ask again.
pub async fn deny_join_request(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Refusal> {
    let lxm = format!("{NS}.denyJoinRequest");
    let asked = body["group"].as_str();
    let (group, _) = doing(&host, &headers, "POST", &lxm, asked, "admit").await?;
    let did = body["member"].as_str().expect("its lexicon requires it");
    let decided = host.store.decide(&group, did);
    if !decided.map_err(XrpcError::internal)? {
        return Err(no_request().into());
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
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    if !host.store.decide(group, &me).map_err(XrpcError::internal)? {
        return Err(no_request());
    }
    Ok(Json(json!({})))
}
