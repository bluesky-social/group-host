//! Acting as the group from the console, and seeing who can.
//!
//! `getGroupAuth` gives a member, signed in as themselves, a short-lived
//! token for the group, bound to their DPoP key, to write in one of its
//! spaces what their roles there may write as the group, and the same
//! kinds of record in its public repo, as a role that may write a kind of
//! record as the group in a space, by its grant there, may publish it. No group
//! password is involved, and there is nothing to refresh.
//!
//! `listGroupTokens` shows a group's configurers every grant that acts as
//! it, whose it is, and what was done with grants lately;
//! `revokeGroupToken` ends one.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::group::members_space;
use crate::group_api::{group_of, signed_in};
use crate::group_invites::{inviting_scope, is_inbox};
use crate::oauth_resource::Refusal;
use crate::oauth_scope::writes_in;
use crate::oauth_store::{Grant, Tokens};
use crate::rbac::{Access, NS};
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use atproto::jose;
use atproto::space::SpaceRef;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// How long a group token lasts.
const GROUP_TOKEN_TTL: i64 = 15 * 60_000;
/// The client a group token is issued to: the host itself, for the member.
pub const GROUP_AUTH_CLIENT: &str = "getGroupAuth";
/// How much of the log `listGroupTokens` shows.
const ACTIVITY_SHOWN: u32 = 20;

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

/// A group token's scope, and the kinds of record it writes in its space
/// and in the group's repo.
type Authed = Result<(String, Vec<String>, Vec<String>), Refusal>;

fn objects(field: &str, items: impl IntoIterator<Item = impl Into<String>>) -> Value {
    items
        .into_iter()
        .map(|i| json!({ field: i.into() }))
        .collect()
}

impl Host {
    /// The kinds of record a holder of `roles` may write as the group in
    /// any of its spaces, and so publish in its repo, each with the
    /// writes they may make of it: `*` for any kind.
    pub fn publishable(
        &self,
        group: &str,
        roles: &[String],
    ) -> Result<BTreeMap<String, BTreeSet<String>>, XrpcError> {
        let entries = self
            .group_records(group, &members_space(group), &format!("{NS}.space"))
            .map_err(XrpcError::internal)?;
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (_, entry) in entries {
            let uri = entry["space"].as_str().unwrap_or_default();
            let access = self.group_record(group, uri, &format!("{NS}.access"), "self");
            let access = access.map_err(XrpcError::internal)?.unwrap_or_default();
            // A space for chosen people is theirs, not the group's to publish.
            let access = Access::of(&access, uri);
            let open = access.people.is_none();
            for (collection, actions) in access.writes(roles).into_iter().filter(|_| open) {
                out.entry(collection).or_default().extend(actions);
            }
        }
        Ok(out)
    }

    /// What a group token for `space`, one of the group's, carries for
    /// `member`: its scope, and the kinds of record it writes there and in
    /// the group's repo.
    fn space_auth(&self, group: &str, member: &str, space: &SpaceRef) -> Authed {
        let (writes, scope) = self.group_scope(group, member, &space.to_string(), space)?;
        let collections: Vec<String> = writes.keys().cloned().collect();
        Ok((scope, collections.clone(), collections))
    }

    /// What a group token for `inbox`, someone's, carries for `member`:
    /// the invites they may write there as the group, if they may invite.
    fn inbox_auth(&self, group: &str, member: &str, inbox: &SpaceRef) -> Authed {
        if !self.may_do(group, member, "invite")? {
            return Err(forbidden("no role of yours may invite").into());
        }
        let invites = vec![format!("{NS}.invite")];
        Ok((inviting_scope(inbox), invites, Vec::new()))
    }

    /// `member`'s standing to act as `group`: may they configure it?
    fn configures(&self, group: &str, member: &str) -> Result<(), XrpcError> {
        group_of(self, group)?;
        if !self.may_do(group, member, "group.configure")? {
            return Err(forbidden("missing group.configure"));
        }
        Ok(())
    }
}

/// The scopes the writes a role holds in `space` become: each collection
/// with its actions, in the space and in the group's repo, and the files
/// those records hold.
fn scopes_for(space: &SpaceRef, writes: &BTreeMap<String, BTreeSet<String>>) -> Vec<String> {
    let mut out = vec!["atproto".to_string()];
    let at = format!(
        "space:{}?authority={}&skey={}",
        space.kind, space.authority, space.skey
    );
    for (collection, actions) in writes {
        let actions: Vec<String> = actions.iter().map(|a| format!("action={a}")).collect();
        let actions = actions.join("&");
        out.push(format!(
            "{at}&collection={collection}&action=read&{actions}"
        ));
        out.push(format!("repo:{collection}?{actions}"));
    }
    let any = writes.contains_key("*");
    out.push(if any { "blob:*/*" } else { "blob:image/*" }.into());
    out
}

impl Host {
    /// What `member`'s roles in `group` may write as it in `space`, at
    /// `uri`: each kind of record with its writes, and the scope of a
    /// token for them.
    pub(crate) fn group_scope(
        &self,
        group: &str,
        member: &str,
        uri: &str,
        space: &SpaceRef,
    ) -> Result<(BTreeMap<String, BTreeSet<String>>, String), XrpcError> {
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let access = self.group_record(group, uri, &format!("{NS}.access"), "self");
        let access = access.map_err(XrpcError::internal)?.unwrap_or_default();
        let everything = self.may_issue(group, member)?;
        let writes = writes_in(&Access::of(&access, uri), &roles, member, everything);
        if writes.is_empty() {
            let none = "no role of yours may act as the group in this space";
            return Err(forbidden(none));
        }
        let scope = scopes_for(space, &writes).join(" ");
        Ok((writes, scope))
    }
}

/// A token to act as the group in one of its spaces, for the caller's
/// DPoP key, as far as their roles there let them write.
pub async fn get_group_auth(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Refusal> {
    let nsid = format!("{NS}.getGroupAuth");
    let me = signed_in(&host, &headers, &nsid).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    let uri = body["space"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    host.active(group)?;
    let parsed = SpaceRef::parse(uri).ok();
    let inbox = parsed.clone().filter(is_inbox);
    let space = parsed.filter(|s| s.authority == group || inbox.is_some());
    let space = space.ok_or_else(|| invalid("not one of this group's spaces"))?;
    let now = now_millis();
    let proof = host.dpop_proof(&headers, "POST", &format!("/xrpc/{nsid}"), None, now);
    let proof = proof.map_err(|e| host.refuse(e.error, e.description))?;
    let proof = proof.ok_or_else(|| host.refuse("invalid_dpop_proof", "DPoP proof required"))?;
    let (scope, collections, published) = match inbox {
        Some(inbox) => host.inbox_auth(group, &me, &inbox)?,
        None => host.space_auth(group, &me, &space)?,
    };
    let access_token = jose::random(32);
    let tokens = Tokens {
        access_hash: &jose::sha256(&access_token),
        access_expires: now + GROUP_TOKEN_TTL,
        // Never handed out: a group token is not refreshed.
        refresh_hash: &jose::sha256(&jose::random(32)),
        expires_at: now + GROUP_TOKEN_TTL,
    };
    let grant = Grant {
        id: format!("tok-{}", jose::random(16)),
        group: group.into(),
        member: me.clone(),
        client_id: GROUP_AUTH_CLIENT.into(),
        client_auth: json!({"method": "none"}),
        scope: scope.clone(),
        dpop_jkt: proof.jkt,
        access_expires: tokens.access_expires,
        expires_at: tokens.expires_at,
        created_at: now,
        source: GROUP_AUTH_CLIENT.into(),
        used_at: None,
    };
    host.store
        .issue(&grant, &tokens)
        .map_err(XrpcError::internal)?;
    let took = format!("took a token for {uri}");
    host.record(group, &me, Some(GROUP_AUTH_CLIENT), &took)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({
        "did": group,
        "accessJwt": access_token,
        "pds": host.config.public_url,
        "collections": objects("collection", collections),
        "repoCollections": objects("collection", published),
        "scope": scope,
        "expiresAt": iso(tokens.expires_at),
    })))
}

/// Every grant that acts as the group, and what grants did lately, for
/// one who configures it.
pub async fn list_group_tokens(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Json<Value>, XrpcError> {
    let me = signed_in(&host, &headers, &format!("{NS}.listGroupTokens")).await?;
    let group = param(&p, "group")?;
    host.configures(group, &me)?;
    let grants = host
        .store
        .grants_of(group, now_millis())
        .map_err(XrpcError::internal)?;
    let sessions: Vec<Value> = grants
        .iter()
        .map(|g| {
            let mut view = json!({
                "tokenId": g.id,
                "clientId": g.client_id,
                "member": g.member,
                "source": g.source,
                "scope": g.scope,
                "grantedAt": iso(g.created_at),
                "expiresAt": iso(g.expires_at),
            });
            if let Some(used) = g.used_at {
                view["lastUsedAt"] = json!(iso(used));
            }
            view
        })
        .collect();
    let log = host
        .store
        .activity(group, ACTIVITY_SHOWN)
        .map_err(XrpcError::internal)?;
    let activity: Vec<Value> = log
        .iter()
        .map(|a| {
            let mut view = json!({"member": a.member, "action": a.action, "at": iso(a.at)});
            if let Some(client) = &a.client_id {
                view["clientId"] = json!(client);
            }
            view
        })
        .collect();
    Ok(Json(json!({"sessions": sessions, "activity": activity})))
}

/// Ends one grant that acts as the group, for one who configures it.
pub async fn revoke_group_token(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, XrpcError> {
    let me = signed_in(&host, &headers, &format!("{NS}.revokeGroupToken")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    let id = body["tokenId"].as_str().expect("its lexicon requires it");
    host.configures(group, &me)?;
    let grant = host.store.grant_by_id(id).map_err(XrpcError::internal)?;
    let Some(grant) = grant.filter(|g| g.group == group) else {
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "NotFound",
            "no such grant of this group",
        ));
    };
    host.store.end_grant(id).map_err(XrpcError::internal)?;
    let ended = format!("revoked {}'s grant to {}", grant.member, grant.client_id);
    host.record(group, &me, Some(&grant.client_id), &ended)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
