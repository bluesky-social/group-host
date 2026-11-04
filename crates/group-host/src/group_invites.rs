//! Invites. A group invites someone by writing an invite in its own repo
//! in the invitee's inbox: a space under the invitee's own DID, at their
//! PDS, which lets this host say who may write there
//! (`com.atproto.managedspace.checkUserAccess`). The invitee's apps read
//! it there; the host keeps its own list of what it sent, which is what
//! redeems an invite, as the inbox is its owner's to read, not the
//! group's.
//!
//! An invite is redeemed by asking to join: admitted at once, whatever
//! the group's join policy, with its default roles.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::group::meta_space;
use crate::group_api::{caller, group_of, signed_in};
use crate::group_members::acting;
use crate::rbac::NS;
use crate::repo::Write;
use crate::store::Invite;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use atproto::syntax;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// The method a space's authority asks this host who may read or write.
pub const CHECK_USER_ACCESS: &str = "com.atproto.managedspace.checkUserAccess";

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// Someone's inbox: where groups' invites to them arrive.
pub fn inbox_of(did: &str) -> String {
    format!("at://{did}/space/{NS}.inbox/self")
}

fn uri_of(invite: &Invite) -> String {
    format!(
        "{}/{}/{NS}.invite/{}",
        inbox_of(&invite.invitee),
        invite.group,
        invite.rkey
    )
}

fn view(invite: &Invite) -> Value {
    let mut view = json!({
        "uri": uri_of(invite),
        "invitee": invite.invitee,
        "createdAt": iso(invite.created_at),
    });
    if let Some(m) = &invite.message {
        view["message"] = json!(m);
    }
    if let Some(at) = invite.expires_at {
        view["expiresAt"] = json!(iso(at));
    }
    view
}

/// An RFC 3339 datetime in UTC (`2026-11-03T00:00:00Z`, fractions of a
/// second allowed), in milliseconds since 1970.
pub fn millis_of(text: &str) -> Option<i64> {
    let text = text.strip_suffix('Z')?;
    let (date, time) = text.split_once('T')?;
    let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
    // Digits alone, of the widths RFC 3339 writes them.
    let number = |part: &str, width: usize| -> Option<i64> {
        let digits = part.len() == width && part.bytes().all(|b| b.is_ascii_digit());
        digits.then(|| part.parse().ok()).flatten()
    };
    let fields = |text: &str, sep: char, widths: &[usize]| -> Option<Vec<i64>> {
        let parts: Vec<&str> = text.split(sep).collect();
        let fits = parts.len() == widths.len();
        fits.then(|| {
            parts
                .iter()
                .zip(widths)
                .map(|(p, w)| number(p, *w))
                .collect()
        })?
    };
    let d = fields(date, '-', &[4, 2, 2])?;
    let t = fields(hms, ':', &[2, 2, 2])?;
    let (y, m, day, h, min, s) = (d[0], d[1], d[2], t[0], t[1], t[2]);
    let frac_ok = frac.bytes().all(|b| b.is_ascii_digit());
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let in_month =
        (1..=12).contains(&m) && (1..=days_in[(m - 1).clamp(0, 11) as usize]).contains(&day);
    if !frac_ok || !in_month || h > 23 || min > 59 || s > 60 {
        return None;
    }
    let ms: i64 = format!("{frac:0<3}")[..3].parse().ok()?;
    // Days since 1970, by the civil calendar.
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + min) * 60_000 + s * 1000 + ms)
}

impl Host {
    /// Takes `invite` back: its record out of the group's repo in the
    /// inbox, and the host's list.
    async fn take_back(&self, invite: &Invite) -> Result<(), XrpcError> {
        let gone = Write::Delete {
            collection: format!("{NS}.invite"),
            rkey: invite.rkey.clone(),
        };
        let inbox = inbox_of(&invite.invitee);
        let written = self.space_write(&invite.group, &inbox, std::slice::from_ref(&gone));
        written.await?;
        let forgot = self.store.uninvite(&invite.group, &invite.invitee);
        forgot.map_err(XrpcError::internal)
    }

    /// `group`'s invite to `did`, if one waits and has not lapsed: taken
    /// back, as it is redeemed.
    pub(crate) async fn redeem(&self, group: &str, did: &str) -> Result<bool, XrpcError> {
        let invited = self
            .store
            .invited(group, did)
            .map_err(XrpcError::internal)?;
        let live = invited.filter(|i| i.expires_at.is_none_or(|at| at > now_millis()));
        match live {
            Some(invite) => {
                self.take_back(&invite).await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

/// Invites someone, for one holding `invite`: an invite written in the
/// group's repo in their inbox, told to their PDS. One already waiting is
/// the one answered.
pub async fn create_invite(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, me, _) = acting(&host, &headers, &body, "createInvite", Some("invite")).await?;
    let invitee = body["invitee"].as_str().unwrap_or_default();
    if !syntax::is_did(invitee) {
        return Err(invalid("invitee is a DID"));
    }
    if members.membership(invitee)?.is_some() {
        return Err(bad("AlreadyMember", "they are a member already"));
    }
    let expires_at = match body["expiresAt"].as_str() {
        Some(at) => Some(millis_of(at).ok_or_else(|| invalid("expiresAt is a datetime"))?),
        None => None,
    };
    let group = members.group.clone();
    // One waiting is the one answered; one lapsed is taken back, and a
    // fresh one written.
    let invited = host
        .store
        .invited(&group, invitee)
        .map_err(XrpcError::internal)?;
    if let Some(waiting) = invited {
        if waiting.expires_at.is_none_or(|at| at > now_millis()) {
            return Ok(Json(json!({ "uri": uri_of(&waiting) })));
        }
        host.take_back(&waiting).await?;
    }
    let invite = Invite {
        seq: 0,
        group: group.clone(),
        invitee: invitee.into(),
        by: me.clone(),
        message: body["message"].as_str().map(String::from),
        rkey: host.clock.next(),
        created_at: now_millis(),
        expires_at,
    };
    let mut record =
        json!({"$type": format!("{NS}.invite"), "by": me, "createdAt": iso(invite.created_at)});
    if let Some(m) = &invite.message {
        record["message"] = json!(m);
    }
    if let Some(at) = body["expiresAt"].as_str() {
        record["expiresAt"] = json!(at);
    }
    let write = Write::Create {
        collection: format!("{NS}.invite"),
        rkey: invite.rkey.clone(),
        value: record,
    };
    let inbox = inbox_of(invitee);
    let written = host.space_write(&group, &inbox, std::slice::from_ref(&write));
    written.await?;
    host.store.invite(&invite).map_err(XrpcError::internal)?;
    Ok(Json(json!({ "uri": uri_of(&invite) })))
}

/// The group's invites waiting, newest first, for one holding `invite`.
pub async fn list_invites(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let body = json!({ "group": param(&p, "group")? });
    let (members, _, _) = acting(&host, &headers, &body, "listInvites", Some("invite")).await?;
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
        .invites(&members.group, before, now_millis(), limit)
        .map_err(XrpcError::internal)?;
    let cursor = waiting
        .last()
        .filter(|_| waiting.len() == limit as usize)
        .map(|i| i.seq.to_string());
    let invites: Vec<Value> = waiting.iter().map(view).collect();
    Ok(Json(json!({"invites": invites, "cursor": cursor})))
}

/// Takes back an invite, by its URI, for one holding `invite`.
pub async fn revoke_invite(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, _, _) = acting(&host, &headers, &body, "revokeInvite", Some("invite")).await?;
    let uri = body["uri"]
        .as_str()
        .ok_or_else(|| invalid("uri is required"))?;
    let invitee = uri.strip_prefix("at://").and_then(|r| r.split('/').next());
    let invitee = invitee
        .filter(|d| syntax::is_did(d))
        .ok_or_else(|| invalid("not an invite's URI"))?;
    let invited = host
        .store
        .invited(&members.group, invitee)
        .map_err(XrpcError::internal)?;
    let invite = invited.ok_or_else(|| bad("InviteNotFound", "no invite to them waits"))?;
    host.take_back(&invite).await?;
    Ok(Json(json!({})))
}

/// The caller's own invites waiting, from the groups here, newest first;
/// with `group`, that group's only.
pub async fn get_actor_invites(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.getActorInvites")).await?;
    let waiting = host
        .store
        .invites_to(&me, now_millis())
        .map_err(XrpcError::internal)?;
    let only = p.get("group");
    let mut invites = Vec::new();
    for invite in waiting
        .iter()
        .filter(|i| only.is_none_or(|g| *g == i.group))
    {
        if host.deactivation(&invite.group)?.is_some() {
            continue;
        }
        let handle = group_of(&host, &invite.group)?;
        let mut group = json!({"did": invite.group, "handle": handle, "address": host.store.address(&invite.group).map_err(XrpcError::internal)?});
        let profile = host
            .group_record(
                &invite.group,
                &meta_space(&invite.group),
                &format!("{NS}.profile"),
                "self",
            )
            .map_err(XrpcError::internal)?
            .unwrap_or_default();
        if let Some(name) = profile.get("displayName") {
            group["displayName"] = name.clone();
        }
        if let Some(cid) = profile["avatar"]["ref"]["$link"].as_str() {
            group["avatar"] = json!(cid);
        }
        let mut shown =
            json!({"group": group, "by": invite.by, "createdAt": iso(invite.created_at)});
        if let Some(m) = &invite.message {
            shown["message"] = json!(m);
        }
        invites.push(shown);
    }
    Ok(Json(json!({ "invites": invites })))
}

/// Turns down the group's invite to the caller: it goes from the group's
/// list and their inbox. The group may invite them again.
pub async fn decline_invite(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.declineInvite")).await?;
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    group_of(&host, group)?;
    let invited = host
        .store
        .invited(group, &me)
        .map_err(XrpcError::internal)?;
    let invite = invited.ok_or_else(|| bad("InviteNotFound", "no invite to you waits"))?;
    host.take_back(&invite).await?;
    Ok(Json(json!({})))
}

/// Whether a user may read or write a space this host manages for its
/// authority: someone's inbox, which its owner reads and groups here
/// write their invites in. Asked by the space's authority, with service
/// auth for this host.
pub async fn check_user_access(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asker = caller(&host, &headers, CHECK_USER_ACCESS).await?;
    let asker = asker
        .ok_or_else(|| crate::space_api::unauthorized("AuthMissing", "service auth is required"))?;
    let space = SpaceRef::parse(param(&p, "space")?).map_err(|e| invalid(e.to_string()))?;
    if asker != space.authority {
        return Err(XrpcError::new(
            StatusCode::FORBIDDEN,
            "Forbidden",
            "only the space's authority asks",
        ));
    }
    let user = param(&p, "user")?;
    let inbox = space.kind == format!("{NS}.inbox");
    let authorized = match p.get("access").map(String::as_str) {
        // Its owner reads it; a group here writes in it.
        Some("write") => {
            inbox
                && host
                    .store
                    .account(user)
                    .map_err(XrpcError::internal)?
                    .is_some_and(|a| a.did == user)
                && group_of(&host, user).is_ok()
        }
        _ => inbox && user == space.authority,
    };
    Ok(Json(json!({ "authorized": authorized })))
}
