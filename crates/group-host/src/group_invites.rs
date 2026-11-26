//! Invites. A group invites someone by writing an invite in its own repo
//! in the invitee's inbox: a space under the invitee's own DID, at their
//! PDS, which their app makes as a memberspace that tracks anyone's
//! writes and names no members, so any group may write there and only its
//! owner reads it. An app writes it as the group, through
//! `com.atproto.space.*` with a session for the group, and takes it back
//! by deleting it; each write is told to the invitee's PDS. The invitee's
//! apps read it there; the host keeps its own list of what it sent, which
//! is what redeems an invite, as the inbox is its owner's to read, not the
//! group's.
//!
//! An invite is redeemed by asking to join: admitted at once, whatever
//! the group's join policy, with its default roles.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::group::meta_space;
use crate::group_api::{group_of, signed_in};
use crate::group_members::{Members, acting};
use crate::lexicons::check_record;
use crate::mcp::McpGrant;
use crate::oauth_resource::Acting;
use crate::oauth_store::Grant;
use crate::rbac::{INTERNAL, NS};
use crate::repo::Write;
use crate::scopes::Permissions;
use crate::store::Invite;
use crate::time::{iso, now_millis};
use crate::token_writes::{action, missing, no_longer};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// Someone's inbox: where groups' invites to them arrive.
pub fn inbox_of(did: &str) -> String {
    format!("at://{did}/space/{NS}.inbox/self")
}

/// Where `invite` is: the group's record in the invitee's inbox.
pub(crate) fn invite_uri(invite: &Invite) -> String {
    format!(
        "{}/{}/{NS}.invite/{}",
        inbox_of(&invite.invitee),
        invite.group,
        invite.rkey
    )
}

fn view(invite: &Invite) -> Value {
    let mut view = json!({
        "uri": invite_uri(invite),
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

/// An RFC 3339 datetime, as atproto writes one: in UTC
/// (`2026-11-03T00:00:00Z`) or at an offset from it (`+01:00`, though not
/// `-00:00`), fractions of a second allowed. In milliseconds since 1970.
pub fn millis_of(text: &str) -> Option<i64> {
    let (text, zone) = match text.strip_suffix('Z') {
        Some(utc) => (utc, "+00:00"),
        None => text.split_at_checked(text.len().checked_sub(6)?)?,
    };
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
    let (sign, offset) = zone.split_at_checked(1)?;
    let o = fields(offset, ':', &[2, 2])?;
    let east = match sign {
        "+" => 1,
        "-" if offset != "00:00" => -1,
        _ => return None,
    };
    if o[0] > 23 || o[1] > 59 {
        return None;
    }
    let east = east * (o[0] * 60 + o[1]) * 60_000;
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
    Some(((days * 24 + h) * 60 + min) * 60_000 + s * 1000 + ms - east)
}

impl Host {
    /// Takes `invite` back: its record out of the group's repo in the
    /// inbox, and the host's list.
    pub(crate) async fn take_back(&self, invite: &Invite) -> Result<(), XrpcError> {
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

    /// `group`'s invite to `did`, if one waits and has not lapsed. One
    /// lapsed is taken back.
    pub(crate) async fn waiting_invite(
        &self,
        group: &str,
        did: &str,
    ) -> Result<Option<Invite>, XrpcError> {
        let invited = self
            .store
            .invited(group, did)
            .map_err(XrpcError::internal)?;
        match invited {
            Some(lapsed) if lapsed.expires_at.is_some_and(|at| at <= now_millis()) => {
                self.take_back(&lapsed).await?;
                Ok(None)
            }
            waiting => Ok(waiting),
        }
    }

    /// Takes back the invites lapsed by `now`, saying how many. One whose
    /// group is switched off waits until it is on again.
    pub async fn sweep_invites(&self, now: i64) -> anyhow::Result<usize> {
        let lapsed = self.store.lapsed_invites(now)?;
        let mut swept = 0;
        for invite in &lapsed {
            if self.take_back(invite).await.is_ok() {
                swept += 1;
            }
        }
        Ok(swept)
    }
}

/// Whether `space` is someone's inbox, where groups write invites to them.
pub fn is_inbox(space: &SpaceRef) -> bool {
    space.kind == format!("{NS}.inbox")
}

/// The scope a group's session takes to write invites in `inbox`, and
/// only there.
pub(crate) fn inviting_scope(inbox: &SpaceRef) -> String {
    format!(
        "atproto space:{NS}.inbox?authority={}&skey={}&collection={NS}.invite&action=create&action=update&action=delete",
        inbox.authority, inbox.skey
    )
}

impl Host {
    /// Makes `writes` in someone's inbox as the group, with `acting`'s
    /// session: one invite at a time, as [`Host::write_invite`] has it.
    pub(crate) async fn write_invites(
        &self,
        acting: &Acting,
        inbox: &SpaceRef,
        writes: Vec<Write>,
    ) -> Result<Vec<Write>, XrpcError> {
        let Ok([write]) = <[Write; 1]>::try_from(writes) else {
            return Err(invalid("an inbox takes one invite write at a time"));
        };
        Ok(vec![self.write_invite(acting, inbox, write).await?])
    }

    /// Writes an invite in the group's repo in someone's inbox, or deletes
    /// one, with `acting`'s session, for a member whose roles hold
    /// `invite`: the write as made, told to the inbox's authority.
    ///
    /// A group has one invite to each person at a time. Written while one
    /// waits, it is that one answered, unless it is written at that one's
    /// key, which replaces it. An invite is for someone not yet a member,
    /// and lapses, if it does, at a time still to come. Deleted, it is
    /// taken back. The host's own list of invites, which redeems them,
    /// keeps in step.
    pub(crate) async fn write_invite(
        &self,
        acting: &Acting,
        inbox: &SpaceRef,
        write: Write,
    ) -> Result<Write, XrpcError> {
        let (group, member) = (&acting.grant.group, &acting.grant.member);
        let kind = format!("{NS}.invite");
        let (collection, rkey) = write.path();
        if collection != kind || inbox.skey != "self" {
            let only = format!("an inbox, at self, holds {kind} records, and no other");
            return Err(invalid(only));
        }
        let asked = action(&write);
        if !acting.permissions.space(inbox, Some(collection), asked) {
            let scope = format!("space:{}?collection={kind}&action={asked}", inbox.kind);
            return Err(missing(scope));
        }
        if !self.may_do(group, member, "invite")? {
            return Err(no_longer(collection));
        }
        let (invitee, uri) = (&inbox.authority, inbox.to_string());
        let waiting = self.waiting_invite(group, invitee).await?;
        let Some(value) = write.value() else {
            let named = waiting.filter(|w| w.rkey == rkey);
            let invite = named.ok_or_else(|| bad("InviteNotFound", "no invite to them waits"))?;
            self.take_back(&invite).await?;
            self.wrote(acting, Some(&uri), std::slice::from_ref(&write))?;
            return Ok(write);
        };
        check_record(collection, value)?;
        if Members::of(self, group)?.membership(invitee)?.is_some() {
            return Err(invalid("they are a member already"));
        }
        let expires_at = match value["expiresAt"].as_str() {
            Some(at) => Some(millis_of(at).ok_or_else(|| invalid("expiresAt is a datetime"))?),
            None => None,
        };
        // One that has lapsed already could admit nobody.
        if expires_at.is_some_and(|at| at <= now_millis()) {
            return Err(invalid("expiresAt is a time to come"));
        }
        if let Some(other) = waiting.as_ref().filter(|w| w.rkey != rkey) {
            let record = self.group_record(group, &uri, &kind, &other.rkey);
            let record = record.map_err(XrpcError::internal)?.unwrap_or_default();
            let (collection, rkey) = (kind, other.rkey.clone());
            return Ok(Write::Create {
                collection,
                rkey,
                value: record,
            });
        }
        let (collection, rkey, value) = (kind, rkey.to_string(), value.clone());
        let message = value["message"].as_str().map(String::from);
        let made = match &waiting {
            Some(_) => Write::Update {
                collection,
                rkey: rkey.clone(),
                value,
            },
            None => Write::Create {
                collection,
                rkey: rkey.clone(),
                value,
            },
        };
        let written = self.space_write(group, &uri, std::slice::from_ref(&made));
        written.await?;
        if waiting.is_some() {
            let replaced = self.store.uninvite(group, invitee);
            replaced.map_err(XrpcError::internal)?;
        }
        let invite = Invite {
            seq: 0,
            group: group.clone(),
            invitee: invitee.clone(),
            by: member.clone(),
            message,
            rkey,
            created_at: now_millis(),
            expires_at,
        };
        self.store.invite(&invite).map_err(XrpcError::internal)?;
        self.wrote(acting, Some(&uri), std::slice::from_ref(&made))?;
        Ok(made)
    }

    /// What an MCP client's `invite_send` does, as the member who connected
    /// it, in their group: the write the member's own group session for
    /// `person`'s inbox would make, `record` written there, or with none
    /// the invite waiting taken back, logged as the MCP client's. The invite's
    /// URI.
    pub(crate) async fn invite_as(
        &self,
        who: &McpGrant,
        person: &str,
        record: Option<Value>,
    ) -> Result<Value, XrpcError> {
        let inbox = SpaceRef::parse(&inbox_of(person)).map_err(|e| invalid(e.to_string()))?;
        let now = now_millis();
        let grant = Grant {
            id: who.id.clone(),
            group: who.group.clone(),
            member: who.member.clone(),
            client_id: who.client_id.clone(),
            client_auth: json!({}),
            scope: inviting_scope(&inbox),
            dpop_jkt: String::new(),
            access_expires: now,
            expires_at: now,
            created_at: now,
            source: "mcp".into(),
            used_at: None,
        };
        let permissions = Permissions::new(&grant.scope, &grant.group);
        let acting = Acting { grant, permissions };
        let collection = format!("{NS}.invite");
        let write = match record {
            Some(value) => Write::Create {
                collection,
                rkey: self.clock.next(),
                value,
            },
            None => {
                let waiting = self.store.invited(&who.group, person);
                let waiting = waiting.map_err(XrpcError::internal)?;
                let rkey = waiting.map(|w| w.rkey).unwrap_or_default();
                Write::Delete { collection, rkey }
            }
        };
        let made = self.write_invite(&acting, &inbox, write).await?;
        let rkey = made.path().1;
        Ok(json!({"uri": format!("{inbox}/{}/{NS}.invite/{rkey}", who.group)}))
    }
}

/// The group's invites waiting, newest first, for one holding `invite`.
pub async fn list_invites(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let body = json!({ "group": param(&p, "group")? });
    let (members, _, _) = acting(
        &host,
        &headers,
        &body,
        &format!("{INTERNAL}.listInvites"),
        Some("invite"),
    )
    .await?;
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
    // Each with whom it invites, by their handle where it can be read.
    let actors = waiting.iter().map(|i| host.actor_view(&i.invitee));
    let actors = futures::future::join_all(actors).await;
    let invites: Vec<Value> = waiting
        .iter()
        .zip(actors)
        .map(|(i, actor)| {
            let mut view = view(i);
            view["actor"] = actor;
            view
        })
        .collect();
    Ok(Json(json!({"invites": invites, "cursor": cursor})))
}

/// The caller's own invites waiting, from the groups here, newest first;
/// with `group`, that group's only.
pub async fn get_actor_invites(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.getActorInvites")).await?;
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
                &format!("{NS}.about"),
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
        let mut shown = json!({"group": group, "createdAt": iso(invite.created_at)});
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
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.declineInvite")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    let invited = host
        .store
        .invited(group, &me)
        .map_err(XrpcError::internal)?;
    let invite = invited.ok_or_else(|| bad("InviteNotFound", "no invite to you waits"))?;
    host.take_back(&invite).await?;
    Ok(Json(json!({})))
}
