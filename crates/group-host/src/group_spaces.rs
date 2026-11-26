//! A group's spaces as their authority keeps them, made, changed, read
//! and ended through atproto's own methods (`com.atproto.space.
//! createSpace`, `updateSpace`, `getSpace`, `deleteSpace`), by a session
//! as the group: an app's, or a group token from `getGroupAuth`. Such a
//! session reads a space as any reader does, with a credential, for which
//! it asks a delegation token here (`getDelegationToken`), as a PDS gives
//! one to an app signed in as one of its accounts.
//!
//! Each space is kept under a policy, `org.atgrp.policy`: for each
//! role, whether its holders read the space, whether they write in it in
//! their own name (and so are in its writer set), and the scopes they hold
//! there acting as the group; whether anyone may read it; and whether it
//! is left out of the group's index. The host keeps the policy, publishes
//! it in the space as the group's `com.atproto.space.policy/self`, and
//! keeps the index, the `org.atgrp.space` entries in the members
//! space, in step with it: both are written by the host alone. A space
//! under any other policy is refused (`UnsupportedPolicy`).
//!
//! Making a space takes `manage=create` in the session's scope and a role
//! granting `space.create`. Changing one takes `manage=update`, and a role
//! granting `space.configure` or a grant in the space's policy managing
//! updates; ending one, `manage=delete` likewise. A change grants no more
//! than its user holds: it changes only the grants of roles they may
//! assign, adds only scopes they hold, and grants reading or writing the
//! space (to a role, or reading to anyone) only to one who reads or
//! writes it. Who loses writing leaves the writer set. The meta and
//! members spaces come and go with the group, and `member`, which every
//! member holds, keeps reading and writing the members space, which
//! nobody else reads (`MembersSpaceFixed`).

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::authority::Access;
use crate::group::{members_space, meta_space};
use crate::group_config::put_in;
use crate::lexicons::lexicons;
use crate::member_writes::{assignable, held};
use crate::oauth_resource::{Acting, Refusal};
use crate::oauth_scope::SpaceScope;
use crate::rbac::{Grant, NS, POLICY, Policy, PolicyGrant, assigns};
use crate::repo::Write;
use crate::scopes::covers;
use crate::space_api::now;
use crate::time::{iso, now_millis};
use crate::token_writes::missing;
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use atproto::space_auth::{self, Kind, Make, space_host_aud};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

type Answer = Result<Json<Value>, Refusal>;

/// The record a space's policy is published as, in the group's repo there.
pub const PUBLISHED: &str = "com.atproto.space.policy";

fn refused(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

fn not_authorized(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "NotAuthorized", message)
}

fn not_found() -> XrpcError {
    refused("SpaceNotFound", "the group has no such space")
}

/// The index's collection.
fn index() -> String {
    format!("{NS}.space")
}

/// Whether `kind` is one of the two space types every group has.
fn reserved(kind: &str) -> bool {
    kind == format!("{NS}.meta") || kind == format!("{NS}.members")
}

/// Refuses `input` to `nsid` unless its lexicon describes it.
fn described(nsid: &str, input: &Value) -> Result<(), XrpcError> {
    let checked = lexicons().validate_input(nsid, input);
    checked.map_err(|e| invalid(e.to_string()))
}

/// The policy `policy` asks for, as kept: of this host's kind, as its
/// lexicon describes it, one grant a role, each of a role the group has,
/// and each scope a space scope.
fn checked_policy(host: &Host, group: &str, policy: &Value) -> Result<Value, XrpcError> {
    if policy["$type"] != POLICY {
        return Err(refused(
            "UnsupportedPolicy",
            format!("a group's spaces are kept under {POLICY}"),
        ));
    }
    let checked = lexicons().validate(POLICY, policy);
    checked.map_err(|e| invalid(e.to_string()))?;
    let roles = host.group_records(group, &members_space(group), &format!("{NS}.role"));
    let roles = roles.map_err(XrpcError::internal)?;
    let mut seen = BTreeSet::new();
    let grants = policy["grants"].as_array().into_iter().flatten();
    for grant in grants {
        let role = grant["role"].as_str().expect("its lexicon requires it");
        if !seen.insert(role) {
            return Err(invalid(format!("{role} has more than one grant")));
        }
        if !roles.iter().any(|(r, _)| r == role) {
            return Err(invalid(format!("the group has no role {role}")));
        }
        let scopes = grant["scopes"].as_array().into_iter().flatten();
        for scope in scopes.filter_map(Value::as_str) {
            if SpaceScope::parse(scope).is_none() {
                return Err(invalid(format!("{scope} is no space scope")));
            }
        }
    }
    Ok(policy.clone())
}

impl Host {
    /// Whether a holder of `roles` reads the space at `uri`, as its policy
    /// has it; none for a space not here.
    pub(crate) fn reads_space(&self, uri: &str, roles: &[String]) -> Result<bool, XrpcError> {
        let policy = self.policy(uri).map_err(XrpcError::internal)?;
        Ok(policy.is_some_and(|p| p.can_read(roles)))
    }

    /// Lists the space at `uri` in `group`'s index, unless it is there.
    async fn index_space(&self, group: &str, uri: &str) -> Result<(), XrpcError> {
        if self.entry_of(group, uri)?.is_some() {
            return Ok(());
        }
        let at = iso(now_millis());
        let entry = json!({"$type": index(), "space": uri, "createdAt": at});
        let listed = Write::Create {
            collection: index(),
            rkey: self.clock.next(),
            value: entry,
        };
        self.space_write(group, &members_space(group), &[listed])
            .await?;
        Ok(())
    }

    /// Takes the space at `uri` out of `group`'s index, if it is there.
    async fn unindex_space(&self, group: &str, uri: &str) -> Result<(), XrpcError> {
        let Some(rkey) = self.entry_of(group, uri)? else {
            return Ok(());
        };
        let gone = Write::Delete {
            collection: index(),
            rkey,
        };
        self.space_write(group, &members_space(group), &[gone])
            .await?;
        Ok(())
    }

    /// The key of `uri`'s entry in `group`'s index, if it has one.
    fn entry_of(&self, group: &str, uri: &str) -> Result<Option<String>, XrpcError> {
        let entries = self.group_records(group, &members_space(group), &index());
        let entries = entries.map_err(XrpcError::internal)?;
        Ok(entries
            .into_iter()
            .find(|(_, e)| e["space"] == uri)
            .map(|(rkey, _)| rkey))
    }

    /// Publishes `policy`, the policy of `group`'s space at `uri`, as the
    /// group's `com.atproto.space.policy/self` there, unless it is so
    /// already, and lists the space in the group's index, or takes it out,
    /// as the policy says.
    pub(crate) async fn publish_policy(
        &self,
        group: &str,
        uri: &str,
        policy: &Value,
    ) -> Result<(), XrpcError> {
        let record = json!({"$type": PUBLISHED, "policy": policy});
        let there = self.group_record(group, uri, PUBLISHED, "self");
        if there.map_err(XrpcError::internal)?.as_ref() != Some(&record) {
            let _published = put_in(self, group, uri, PUBLISHED, "self", record).await?;
        }
        match policy["unlisted"] == true {
            true => self.unindex_space(group, uri).await,
            false => self.index_space(group, uri).await,
        }
    }

    /// Makes the space `body` asks for (`com.atproto.space.createSpace`)
    /// as `acting`, for a member whose roles may make spaces: kept under
    /// its policy, published, and listed unless unlisted. Its URI.
    pub(crate) async fn create_space_as(
        &self,
        acting: &Acting,
        body: &Value,
    ) -> Result<String, XrpcError> {
        described("com.atproto.space.createSpace", body)?;
        let (group, member) = (&acting.grant.group, &acting.grant.member);
        self.active(group)?;
        let kind = body["spaceType"].as_str().expect("its lexicon requires it");
        let skey = match body["skey"].as_str() {
            Some(skey) => skey.to_string(),
            None => self.clock.next(),
        };
        if reserved(kind) {
            return Err(invalid("the meta and members spaces come with the group"));
        }
        let space = SpaceRef {
            authority: group.clone(),
            kind: kind.into(),
            skey: skey.clone(),
        };
        if !acting.permissions.manages(&space, "create") {
            return Err(missing(format!("space:{kind}?manage=create")));
        }
        if !self.may_do(group, member, "space.create")? {
            return Err(not_authorized("no role of yours may make spaces"));
        }
        let policy = checked_policy(self, group, &body["policy"])?;
        let uri = self.create_space(group, kind, &skey, &policy)?;
        // A space whose policy is not published could not be read for what
        // it is: it is made whole or not at all.
        if let Err(e) = self.publish_policy(group, &uri, &policy).await {
            self.end_space(group, &uri).map_err(XrpcError::internal)?;
            return Err(e);
        }
        let logged = self.record(
            group,
            member,
            Some(&acting.grant.client_id),
            &format!("created space {uri}"),
        );
        logged.map_err(XrpcError::internal)?;
        Ok(uri)
    }

    /// The space `uri` names, `group`'s, as kept: or `SpaceNotFound`.
    fn own(&self, group: &str, uri: &str) -> Result<(SpaceRef, Policy), XrpcError> {
        let space = SpaceRef::parse(uri).map_err(|e| invalid(e.to_string()))?;
        let policy = self.policy(uri).map_err(XrpcError::internal)?;
        let policy = policy.filter(|_| space.authority == group);
        Ok((space, policy.ok_or_else(not_found)?))
    }

    /// Whether `member`, holding `roles` under `grants`, may `op` (`update`,
    /// `delete`) the space under `policy`: by a role granting it across
    /// the group (`action`), or a grant in its policy.
    fn may_manage(
        &self,
        group: &str,
        member: &str,
        policy: &Policy,
        (roles, action, op): (&[String], &str, &str),
    ) -> Result<bool, XrpcError> {
        Ok(self.may_do(group, member, action)? || policy.manages(roles, op))
    }

    /// Puts the space `body` names under the policy it asks for
    /// (`com.atproto.space.updateSpace`), as `acting`, for a member who
    /// may change it, and grants no more than they hold.
    pub(crate) async fn update_space_as(
        &self,
        acting: &Acting,
        body: &Value,
    ) -> Result<(), XrpcError> {
        described("com.atproto.space.updateSpace", body)?;
        let (group, member) = (&acting.grant.group, &acting.grant.member);
        self.active(group)?;
        let uri = body["space"].as_str().expect("its lexicon requires it");
        let (space, now) = self.own(group, uri)?;
        if !acting.permissions.manages(&space, "update") {
            return Err(missing(format!("space:{}?manage=update", space.kind)));
        }
        let (roles, grants) = self.standing(acting)?;
        let asked = (&roles[..], "space.configure", "update");
        if !self.may_manage(group, member, &now, asked)? {
            return Err(not_authorized("no role of yours may change this space"));
        }
        let policy = checked_policy(self, group, &body["policy"])?;
        let next = Policy::of(&policy, &space);
        // Every member holds `member`, which reads the members space and
        // writes there in its own name: how members know the group and
        // each other, and take up their place. Nobody else reads it.
        let kept = grant_of(&next, "member");
        let fixed = kept.read && kept.write && !next.public_read;
        if uri == members_space(group) && !fixed {
            return Err(refused(
                "MembersSpaceFixed",
                "member reads and writes in the members space, and nobody else reads it",
            ));
        }
        escalates(group, &roles, &grants, &now, &next)?;
        self.store
            .set_policy(uri, &policy)
            .map_err(XrpcError::internal)?;
        self.publish_policy(group, uri, &policy).await?;
        // The group is declared while anyone may read its face.
        if uri == meta_space(group) {
            self.declare(group, next.public_read).await?;
        }
        let hosted = self.hosted_space(uri)?;
        let pruned = self.prune_writers(uri, &hosted, None);
        pruned.map_err(XrpcError::internal)?;
        // An app's grant whose member reads it no more ends its reads.
        let ended = self.end_grant_reads(group).await;
        ended.map_err(XrpcError::internal)?;
        let logged = self.record(
            group,
            member,
            Some(&acting.grant.client_id),
            &format!("updated space {uri}"),
        );
        logged.map_err(XrpcError::internal)
    }

    /// Ends the space `body` names (`com.atproto.space.deleteSpace`), as
    /// `acting`, for a member who may: out of the index, its syncers told,
    /// its authority's state and the group's own repo in it gone. What
    /// members wrote there is theirs, at their own PDSes.
    pub(crate) async fn delete_space_as(
        &self,
        acting: &Acting,
        body: &Value,
    ) -> Result<(), XrpcError> {
        described("com.atproto.space.deleteSpace", body)?;
        let (group, member) = (&acting.grant.group, &acting.grant.member);
        self.active(group)?;
        let uri = body["space"].as_str().expect("its lexicon requires it");
        let (space, now) = self.own(group, uri)?;
        if reserved(&space.kind) {
            return Err(not_authorized(
                "the meta and members spaces come and go with the group",
            ));
        }
        if !acting.permissions.manages(&space, "delete") {
            return Err(missing(format!("space:{}?manage=delete", space.kind)));
        }
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let asked = (&roles[..], "space.delete", "delete");
        if !self.may_manage(group, member, &now, asked)? {
            return Err(not_authorized("no role of yours may end this space"));
        }
        self.unindex_space(group, uri).await?;
        self.end_space(group, uri).map_err(XrpcError::internal)?;
        let logged = self.record(
            group,
            member,
            Some(&acting.grant.client_id),
            &format!("deleted space {uri}"),
        );
        logged.map_err(XrpcError::internal)
    }
}

/// The grant `role` has in `policy`, or none at all.
fn grant_of(policy: &Policy, role: &str) -> PolicyGrant {
    let none = || PolicyGrant {
        role: role.into(),
        ..PolicyGrant::default()
    };
    policy.grant(role).cloned().unwrap_or_else(none)
}

/// Refuses a policy `next`, in place of `now`, by a holder of `roles`
/// under `grants`, that changes the grant of a role they may not assign,
/// adds a scope they do not hold, or grants reading or writing the space
/// (to a role, or reading to anyone) that they do not have there.
fn escalates(
    group: &str,
    roles: &[String],
    grants: &[Grant],
    now: &Policy,
    next: &Policy,
) -> Result<(), XrpcError> {
    let union = assignable(grants, roles, None);
    let mut mine = held(grants, roles);
    mine.extend(now.scopes(roles));
    let named: BTreeSet<&String> = now
        .grants
        .iter()
        .chain(&next.grants)
        .map(|g| &g.role)
        .collect();
    if next.public_read && !now.public_read && !now.can_read(roles) {
        return Err(not_authorized("you do not read this space, to let anyone"));
    }
    for role in named {
        let (before, after) = (grant_of(now, role), grant_of(next, role));
        if before == after {
            continue;
        }
        if !assigns(&union, role) {
            return Err(not_authorized(format!(
                "not yours to change: {role}'s grant"
            )));
        }
        if after.read && !before.read && !now.can_read(roles) {
            return Err(not_authorized("you do not read this space, to let others"));
        }
        if after.write && !before.write && !now.writes_own(roles) {
            return Err(not_authorized(
                "you do not write in this space, to let others",
            ));
        }
        let added = after.scopes.iter().filter(|s| !before.scopes.contains(s));
        for scope in added {
            if !covers(&mine, scope, group) {
                return Err(not_authorized(format!(
                    "your roles do not hold {scope}, to give it"
                )));
            }
        }
    }
    Ok(())
}

/// `com.atproto.space.createSpace`, as the group.
pub async fn create_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.acting(&headers, "POST", "/xrpc/com.atproto.space.createSpace")?;
    let uri = host.create_space_as(&acting, &body).await?;
    Ok(Json(json!({ "space": uri })))
}

/// `com.atproto.space.updateSpace`, as the group.
pub async fn update_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.acting(&headers, "POST", "/xrpc/com.atproto.space.updateSpace")?;
    host.update_space_as(&acting, &body).await?;
    Ok(Json(json!({})))
}

/// `com.atproto.space.deleteSpace`, as the group.
pub async fn delete_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.acting(&headers, "POST", "/xrpc/com.atproto.space.deleteSpace")?;
    host.delete_space_as(&acting, &body).await?;
    Ok(Json(json!({})))
}

/// `com.atproto.space.getSpace`, as the group: a space and its policy, to
/// a session that reads it, for a member who reads it or may change it.
pub async fn get_space(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let acting = host.acting(&headers, "GET", "/xrpc/com.atproto.space.getSpace")?;
    let (group, member) = (&acting.grant.group, &acting.grant.member);
    let uri = param(&p, "space")?;
    let (space, policy) = host.own(group, uri)?;
    if !acting.permissions.space(&space, None, "read_self") {
        return Err(missing(format!("space:{}?action=read_self", space.kind)).into());
    }
    let roles = host.roles(group, member).map_err(XrpcError::internal)?;
    let asked = (&roles[..], "space.configure", "update");
    if !policy.can_read(&roles) && !host.may_manage(group, member, &policy, asked)? {
        return Err(not_authorized("you do not read this space").into());
    }
    let kept = host.hosted_space(uri)?;
    Ok(Json(json!({"space": uri, "policy": kept.policy})))
}

/// `com.atproto.space.getDelegationToken`, as the group: a delegation
/// token the group signs, for the space's authority (this host) to give a
/// credential for, as a PDS gives one to an app signed in as its account.
/// For one of the group's spaces, to a session whose scope reads it, while
/// its member may read it themselves: so a session never reads more than
/// its member, at each credential. The token is kept as the grant's, so
/// what it buys ends with the grant, or with its member's reading.
pub async fn get_delegation_token(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let path = "/xrpc/com.atproto.space.getDelegationToken";
    let acting = host.acting(&headers, "GET", path)?;
    let (group, member) = (&acting.grant.group, &acting.grant.member);
    let uri = param(&p, "space")?;
    let (space, _) = host.own(group, uri)?;
    if !acting.permissions.space(&space, None, "read") {
        return Err(missing(format!("space:{}?action=read", space.kind)).into());
    }
    let hosted = host.hosted_space(uri)?;
    let reads = host.may(uri, &hosted, Some(member), Access::Read);
    if !reads.map_err(XrpcError::internal)? {
        return Err(not_authorized("you do not read this space").into());
    }
    let account = host.store.account(group).map_err(XrpcError::internal)?;
    let account = account.ok_or_else(not_found)?;
    let (sub, aud) = (space.to_string(), space_host_aud(group));
    let make = Make {
        iss: group,
        sub: &sub,
        aud: Some(&aud),
        ..Make::default()
    };
    let token = space_auth::create(Kind::Delegation, &make, &account.signing, now());
    let token = token.map_err(XrpcError::internal)?;
    let (_, claims) = space_auth::parse(Kind::Delegation, &token).map_err(XrpcError::internal)?;
    let kept = host.store.delegated(
        &claims.jti,
        &acting.grant.id,
        claims.exp as i64,
        now() as i64,
    );
    kept.map_err(XrpcError::internal)?;
    Ok(Json(json!({ "token": token })))
}
