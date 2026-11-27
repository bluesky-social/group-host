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
//! it, whose it is, and what was done with grants lately: each app as its
//! client metadata names it (`apps.rs`), each write with what its kind of
//! record is as its lexicon describes it (`permission_sets.rs`), and each
//! change to a space as what it did. `revokeGroupToken` ends one.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::group::{members_space, meta_space};
use crate::group_api::{group_of, signed_in};
use crate::group_invites::{inviting_scope, is_inbox};
use crate::member_writes::own_scopes;
use crate::oauth_resource::Refusal;
use crate::oauth_scope::{may_write, with_labels};
use crate::oauth_store::{Grant, Tokens};
use crate::rbac::{INTERNAL, LABEL, NS, Policy, label_writes, meta_writes};
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use atproto::jose;
use atproto::space::SpaceRef;
use atproto::syntax::is_nsid;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
        let spaces = self.store.spaces_of(group).map_err(XrpcError::internal)?;
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for uri in spaces {
            let policy = self.policy(&uri).map_err(XrpcError::internal)?;
            let policy = policy.expect("a space the group has");
            for (collection, actions) in policy.writes(roles) {
                out.entry(collection).or_default().extend(actions);
            }
        }
        Ok(out)
    }

    /// What a group token for `space`, one of the group's, carries for
    /// `member`: its scope, and the kinds of record it writes there and in
    /// the group's repo.
    fn space_auth(&self, group: &str, member: &str, space: &SpaceRef) -> Authed {
        let (writes, own, scope) = self.group_scope(group, member, &space.to_string(), space)?;
        let published: Vec<String> = writes.keys().cloned().collect();
        let collections = published.iter().chain(own.keys()).cloned().collect();
        Ok((scope, collections, published))
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

/// Kinds of record, each with the writes that may be made of it.
type Writable = BTreeMap<String, BTreeSet<String>>;

impl Host {
    /// What `member`'s roles in `group` may write as it in `space`, at
    /// `uri`: each kind of record with its writes, as the space's policy
    /// grants them and, in the meta and members spaces, as the group
    /// grants them its own records there; and the scope of a token for
    /// them, and for what they may do to the space itself (making it, if
    /// it is not yet made, changing and ending it).
    pub(crate) fn group_scope(
        &self,
        group: &str,
        member: &str,
        uri: &str,
        space: &SpaceRef,
    ) -> Result<(Writable, Writable, String), XrpcError> {
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let grants = self.role_grants(group).map_err(XrpcError::internal)?;
        let policy = self.policy(uri).map_err(XrpcError::internal)?;
        let writes = policy
            .as_ref()
            .map(|p| p.writes(&roles))
            .unwrap_or_default();
        let mut own = if uri == members_space(group) {
            self.members_writes_of(group, &roles)?
        } else if uri == meta_space(group) {
            meta_writes(&grants, &roles, group)
        } else {
            BTreeMap::new()
        };
        // Labels, as far as what the space grants does not hold them.
        if policy.is_some() {
            let labels = label_writes(&grants, &roles, group, space);
            if !labels.iter().all(|w| may_write(&writes, LABEL, w)) {
                with_labels(&mut own, labels);
            }
        }
        let manage = self.manage_ops(group, member, &roles, policy.as_ref())?;
        if writes.is_empty() && own.is_empty() && manage.is_empty() {
            let none = "no role of yours may act as the group in this space";
            return Err(forbidden(none));
        }
        let mut scope = scopes_for(space, &writes);
        scope.extend(own_scopes(space, &own));
        if !manage.is_empty() {
            let ops: Vec<String> = manage.iter().map(|op| format!("&manage={op}")).collect();
            scope.push(format!(
                "space:{}?authority={}&skey={}&action=read{}",
                space.kind,
                space.authority,
                space.skey,
                ops.concat()
            ));
        }
        Ok((writes, own, scope.join(" ")))
    }

    /// What `member`, holding `roles`, may do to the space under `policy`
    /// itself, or, if there is none yet, make it: by their roles across
    /// the group, or the policy's grants.
    fn manage_ops(
        &self,
        group: &str,
        member: &str,
        roles: &[String],
        policy: Option<&Policy>,
    ) -> Result<Vec<&'static str>, XrpcError> {
        let Some(policy) = policy else {
            let creates = self.may_do(group, member, "space.create")?;
            return Ok(creates.then_some("create").into_iter().collect());
        };
        let mut ops = Vec::new();
        for (op, action) in [("update", "space.configure"), ("delete", "space.delete")] {
            if self.may_do(group, member, action)? || policy.manages(roles, op) {
                ops.push(op);
            }
        }
        Ok(ops)
    }
}

/// A token to act as the group in one of its spaces, for the caller's
/// DPoP key, as far as their roles there let them write.
pub async fn get_group_auth(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Refusal> {
    let nsid = format!("{INTERNAL}.getGroupAuth");
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

/// What a write in the group's log was, as its words say it (`created
/// <collection>/<rkey> in <space>`): its operation, the record's
/// collection and key, the space, if it was in one, and the DID of whom
/// it is about, if any: a record kept under someone's DID (a
/// membership), or one written in their inbox (an invite). Nothing, for
/// what was not a write, a change to a space among it.
fn write_of(action: &str) -> Option<(Value, Option<String>)> {
    let (done, rest) = action.split_once(' ')?;
    let at = ["created", "updated", "deleted"]
        .iter()
        .position(|d| *d == done)?;
    let (path, space) = rest
        .split_once(" in ")
        .map_or((rest, None), |(p, s)| (p, Some(s)));
    let (collection, rkey) = path.split_once('/').filter(|(c, _)| is_nsid(c))?;
    let inbox = space.and_then(|s| SpaceRef::parse(s).ok()).filter(is_inbox);
    let about = rkey.starts_with("did:").then(|| rkey.to_string());
    let about = about.or(inbox.map(|s| s.authority));
    let operation = ["create", "update", "delete"][at];
    let mut write = json!({
        "operation": operation,
        "collection": collection,
        "rkey": rkey,
    });
    if let Some(space) = space {
        write["space"] = json!(space);
    }
    Some((write, about))
}

/// The words the log says a change to one of the group's spaces in, each
/// with its operation, and, for one that opened it to anyone or closed it
/// to those its policy names, whether anyone may read it now.
const SPACE_CHANGES: [(&str, &str, Option<bool>); 5] = [
    ("created", "create", None),
    ("updated", "update", None),
    ("deleted", "delete", None),
    ("opened", "update", Some(true)),
    ("closed", "update", Some(false)),
];

/// What a change to one of the group's spaces in its log was, as its
/// words say it (`opened space <uri>`): the space, the operation, and
/// whether anyone may read it now, where that changed. Nothing, for what
/// was not one.
fn space_of(action: &str) -> Option<Value> {
    let (done, uri) = action.split_once(" space ")?;
    let (_, operation, public_read) = SPACE_CHANGES.iter().find(|(d, ..)| *d == done)?;
    let mut view = json!({"space": uri, "operation": operation});
    if let Some(public_read) = public_read {
        view["publicRead"] = json!(public_read);
    }
    Some(view)
}

impl Host {
    /// The apps `ids` name, as a page shows them, where their metadata can
    /// be read: each read once.
    async fn client_views<'a>(
        &self,
        ids: impl Iterator<Item = &'a str>,
    ) -> HashMap<&'a str, Value> {
        let now = now_millis();
        let ids: BTreeSet<&str> = ids.collect();
        let read = ids
            .into_iter()
            .map(async |id| (id, self.client_view(id, now).await));
        let read = futures::future::join_all(read).await;
        read.into_iter()
            .filter_map(|(id, v)| Some((id, v?)))
            .collect()
    }

    /// What each kind of record among `collections` is, as its lexicon
    /// describes it, where that can be read: each read once.
    async fn descriptions<'a>(
        &self,
        collections: impl Iterator<Item = &'a str>,
    ) -> HashMap<String, String> {
        let collections: BTreeSet<&str> = collections.collect();
        let read = collections
            .into_iter()
            .map(async |c| (c.to_string(), self.lexicon_description(c).await));
        let read = futures::future::join_all(read).await;
        read.into_iter()
            .filter_map(|(c, d)| Some((c, d?)))
            .collect()
    }
}

/// Every grant that acts as the group, and what grants did lately, for
/// one who configures it.
pub async fn list_group_tokens(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Json<Value>, XrpcError> {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.listGroupTokens")).await?;
    let group = param(&p, "group")?;
    host.configures(group, &me)?;
    let grants = host
        .store
        .grants_of(group, now_millis())
        .map_err(XrpcError::internal)?;
    let log = host
        .store
        .activity(group, ACTIVITY_SHOWN)
        .map_err(XrpcError::internal)?;
    let writes: Vec<_> = log.iter().map(|a| write_of(&a.action)).collect();
    // Whom each names, by their handle where it can be read.
    let members = grants.iter().map(|g| g.member.as_str());
    let members = members.chain(log.iter().map(|a| a.member.as_str()));
    let abouts = writes
        .iter()
        .flatten()
        .filter_map(|(_, about)| about.as_deref());
    let actors = host.actor_views(members.chain(abouts)).await;
    // The apps, as their metadata names them, and the kinds of record
    // written, as their lexicons describe them.
    let ids = grants.iter().map(|g| g.client_id.as_str());
    let ids = ids.chain(log.iter().filter_map(|a| a.client_id.as_deref()));
    let clients = host.client_views(ids).await;
    let collections = writes.iter().flatten();
    let collections = collections.filter_map(|(w, _)| w["collection"].as_str());
    let descriptions = host.descriptions(collections).await;
    let sessions: Vec<Value> = grants
        .iter()
        .map(|g| {
            let mut view = json!({
                "tokenId": g.id,
                "clientId": g.client_id,
                "member": g.member,
                "actor": actors[&g.member],
                "source": g.source,
                "scope": g.scope,
                "grantedAt": iso(g.created_at),
                "expiresAt": iso(g.expires_at),
            });
            if let Some(used) = g.used_at {
                view["lastUsedAt"] = json!(iso(used));
            }
            if let Some(client) = clients.get(g.client_id.as_str()) {
                view["client"] = client.clone();
            }
            view
        })
        .collect();
    let activity: Vec<Value> = log
        .iter()
        .zip(writes)
        .map(|(a, write)| {
            let actor = &actors[&a.member];
            let mut view =
                json!({"member": a.member, "actor": actor, "action": a.action, "at": iso(a.at)});
            if let Some(client) = &a.client_id {
                view["clientId"] = json!(client);
                if let Some(app) = clients.get(client.as_str()) {
                    view["client"] = app.clone();
                }
            }
            // A write, said as its parts, with what its kind of record
            // is, and whom it is about.
            if let Some((mut write, about)) = write {
                let kind = write["collection"].as_str();
                if let Some(said) = kind.and_then(|c| descriptions.get(c)) {
                    write["description"] = json!(said);
                }
                view["write"] = write;
                if let Some(about) = about {
                    view["subject"] = actors[&about].clone();
                }
            }
            // A change to one of the group's spaces.
            if let Some(space) = space_of(&a.action) {
                view["space"] = space;
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
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.revokeGroupToken")).await?;
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
    // What it bought of the group's spaces ends with it.
    let bought = host.end_grant_reads(group).await;
    bought.map_err(XrpcError::internal)?;
    let ended = format!("revoked {}'s grant to {}", grant.member, grant.client_id);
    host.record(group, &me, Some(&grant.client_id), &ended)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
