//! A group's roles and who holds them: defining a role and what it may
//! do, giving people roles, putting someone out, and leaving.
//!
//! Roles and memberships are records in the members space, written by the
//! group, so the host writes them for a caller whose roles allow it. A
//! group never loses its last way to be run: the last role able to
//! configure it keeps that, and its last holder may not leave.
//!
//! Someone who stops being a member reads its spaces no more: the
//! credentials they hold are revoked, and the index forgets what they
//! wrote there.
//!
//! As the prototype's `methods/roles.ts`, `methods/membership.ts` and
//! `leaveGroup` in `methods/join.ts`.

use crate::Host;
use crate::api::invalid;
use crate::group::{did_rkey, members_space};
use crate::group_api::{group_of, signed_in};
use crate::group_config::put_in;
use crate::rbac::{ACTIONS, Grant, NS, actions_for, actions_of, grants, listed, scope_of};
use crate::repo::Write;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

/// A group's members space, its grants, and its records there.
pub(crate) struct Members<'a> {
    pub host: &'a Host,
    pub group: String,
    pub space: String,
    pub grants: Vec<Grant>,
}

impl<'a> Members<'a> {
    pub(crate) fn of(host: &'a Host, group: &str) -> Result<Members<'a>, XrpcError> {
        let space = members_space(group);
        let permissions = host
            .group_record(group, &space, &format!("{NS}.permissions"), "self")
            .map_err(XrpcError::internal)?;
        Ok(Members {
            host,
            group: group.into(),
            space,
            grants: grants(&permissions.unwrap_or_default()),
        })
    }

    pub(crate) fn records(&self, collection: &str) -> Result<Vec<(String, Value)>, XrpcError> {
        self.host
            .group_records(&self.group, &self.space, &format!("{NS}.{collection}"))
            .map_err(XrpcError::internal)
    }

    /// `did`'s roles, and whether they are a member at all.
    pub(crate) fn membership(&self, did: &str) -> Result<Option<Vec<String>>, XrpcError> {
        let record = self
            .host
            .group_record(
                &self.group,
                &self.space,
                &format!("{NS}.membership"),
                &did_rkey(did),
            )
            .map_err(XrpcError::internal)?;
        Ok(record.map(|m| listed(&m, "roles", "role")))
    }

    pub(crate) fn can(&self, roles: &[String], action: &str) -> bool {
        actions_for(&self.grants, roles).contains(action)
    }

    /// The roles holders of `roles` may give or take for `action`.
    pub(crate) fn assignable(&self, roles: &[String], action: &str) -> BTreeSet<String> {
        self.grants
            .iter()
            .filter(|g| roles.contains(&g.role) && actions_of(&g.scopes).contains(action))
            .flat_map(|g| g.assignable.iter().cloned())
            .collect()
    }

    /// The roles every new member gets: those the group marks so, or
    /// `member` in a group that marks none.
    pub(crate) fn default_roles(&self) -> Result<Vec<String>, XrpcError> {
        let marked: Vec<String> = self
            .records("role")?
            .into_iter()
            .filter(|(_, r)| r["default"] == true)
            .map(|(k, _)| k)
            .collect();
        Ok(if marked.is_empty() {
            vec!["member".into()]
        } else {
            marked
        })
    }

    /// Writes `did`'s membership, holding `roles`.
    pub(crate) async fn put_membership(
        &self,
        did: &str,
        roles: &[String],
    ) -> Result<(), XrpcError> {
        let membership = json!({
            "$type": format!("{NS}.membership"),
            "member": did,
            "roles": objects("role", roles),
            "createdAt": iso(now_millis()),
        });
        let (collection, key) = (format!("{NS}.membership"), did_rkey(did));
        let put = put_in(
            self.host,
            &self.group,
            &self.space,
            &collection,
            &key,
            membership,
        );
        let _written = put.await?;
        Ok(())
    }

    /// Refuses a change that would leave nobody able to configure the
    /// group: `who` holding `roles` after it, or out of it with none.
    fn keeps_a_configurer(&self, who: &str, roles: &[String]) -> Result<(), XrpcError> {
        let configuring = self.configuring();
        let theirs = roles.iter().any(|r| configuring.contains(r));
        let others = self.records("membership")?.into_iter().any(|(_, m)| {
            m["member"] != who
                && listed(&m, "roles", "role")
                    .iter()
                    .any(|r| configuring.contains(r))
        });
        if theirs || others {
            return Ok(());
        }
        Err(bad(
            "LastConfigurer",
            "nobody would be left who can run this group: give that to someone else first",
        ))
    }

    /// The roles whose grant lets their holders configure the group.
    pub(crate) fn configuring(&self) -> BTreeSet<String> {
        self.grants
            .iter()
            .filter(|g| actions_of(&g.scopes).contains("group.configure"))
            .map(|g| g.role.clone())
            .collect()
    }

    pub(crate) fn permissions(&self, grants: &[Grant]) -> Value {
        let grants: Vec<Value> = grants.iter().map(Grant::to_json).collect();
        json!({"$type": format!("{NS}.permissions"), "grants": grants, "createdAt": iso(now_millis())})
    }
}

/// The group a body names and its members space, once the caller of
/// `lxm` may do `action` in it; and the caller's roles.
pub(crate) async fn acting<'a>(
    host: &'a Host,
    headers: &HeaderMap,
    body: &Value,
    lxm: &str,
    action: Option<&str>,
) -> Result<(Members<'a>, String, Vec<String>), XrpcError> {
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    group_of(host, group)?;
    let members = Members::of(host, group)?;
    let roles = members.membership(&me)?.unwrap_or_default();
    if let Some(action) = action
        && !members.can(&roles, action)
    {
        return Err(forbidden(format!("missing {action}")));
    }
    Ok((members, me, roles))
}

fn objects(field: &str, items: &[String]) -> Value {
    items.iter().map(|i| json!({ field: i })).collect()
}

/// Defines a role, or changes one: its name, words, whether its people
/// are bots or every new member gets it, and what it may do. For one
/// holding `group.configure`.
pub async fn put_role(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, _, _) =
        acting(&host, &headers, &body, "putRole", Some("group.configure")).await?;
    let role = body["role"].as_str().unwrap_or_default().trim();
    if role.is_empty() || role.len() > 64 || !atproto::syntax::is_record_key(role) {
        return Err(invalid("role is a key of 64 characters at most"));
    }
    let name = body["name"]
        .as_str()
        .ok_or_else(|| invalid("name is required"))?;
    let actions: Vec<String> = body["actions"]
        .as_array()
        .ok_or_else(|| invalid("actions is required"))?
        .iter()
        .map(|a| a.as_str().unwrap_or_default().to_string())
        .collect();
    let unknown: Vec<&String> = actions
        .iter()
        .filter(|a| !ACTIONS.contains(&a.as_str()))
        .collect();
    if !unknown.is_empty() {
        return Err(invalid(format!("unknown actions: {unknown:?}")));
    }
    for flag in ["bot", "default"] {
        if body.get(flag).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid(format!("{flag} is true or false")));
        }
    }
    // The last role able to configure the group keeps that, or nobody
    // could run it again.
    let configuring = members.configuring();
    let last = configuring.len() == 1 && configuring.contains(role);
    if last && !actions.iter().any(|a| a == "group.configure") {
        return Err(bad(
            "LastConfigurer",
            "the only role that configures the group keeps that",
        ));
    }
    let collection = format!("{NS}.role");
    let was = members
        .records("role")?
        .into_iter()
        .find(|(k, _)| k == role)
        .map(|(_, r)| r);
    let mut record =
        json!({"$type": collection, "displayName": name, "createdAt": iso(now_millis())});
    if let Some(d) = body["description"].as_str() {
        record["description"] = json!(d);
    }
    // Left out, whether it is given on joining stays as it was.
    let given = body["default"]
        .as_bool()
        .or_else(|| was.as_ref().and_then(|w| w["default"].as_bool()));
    if given == Some(true) {
        record["default"] = json!(true);
    }
    if body["bot"].as_bool() == Some(true) {
        record["bot"] = json!(true);
    }
    // Its grant: the actions as their scopes, beside any other scope it
    // held; its public-repo collections as asked, or as they were.
    let previous = members.grants.iter().find(|g| g.role == role);
    let kept = previous.map(|g| g.scopes.clone()).unwrap_or_default();
    let asked_collections = body
        .get("repoCollections")
        .map(|_| listed(&body, "repoCollections", "collection"));
    let mut scopes: Vec<String> = kept
        .into_iter()
        .filter(|s| actions_of(std::slice::from_ref(s)).is_empty())
        .filter(|s| asked_collections.is_none() || !s.starts_with("repo:"))
        .collect();
    scopes.extend(actions.iter().filter_map(|a| scope_of(a)));
    scopes.extend(
        asked_collections
            .unwrap_or_default()
            .iter()
            .map(|c| format!("repo:{c}")),
    );
    let grant = Grant {
        role: role.into(),
        scopes,
        assignable: listed(&body, "assignable", "role"),
    };
    let mut granted: Vec<Grant> = members
        .grants
        .iter()
        .filter(|g| g.role != role)
        .cloned()
        .collect();
    granted.push(grant);
    let space = members.space.clone();
    let put = put_in(&host, &members.group, &space, &collection, role, record);
    let answer = put.await?;
    let permissions = members.permissions(&granted);
    let permissions_collection = format!("{NS}.permissions");
    let put = put_in(
        &host,
        &members.group,
        &space,
        &permissions_collection,
        "self",
        permissions,
    );
    let _written = put.await?;
    Ok(answer)
}

/// Takes a role away, once nobody holds it. For one holding
/// `group.configure`.
pub async fn delete_role(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, _, _) = acting(
        &host,
        &headers,
        &body,
        "deleteRole",
        Some("group.configure"),
    )
    .await?;
    let role = body["role"]
        .as_str()
        .ok_or_else(|| invalid("role is required"))?;
    let configuring = members.configuring();
    if configuring.len() == 1 && configuring.contains(role) {
        return Err(bad(
            "LastConfigurer",
            "the only role that configures the group stays",
        ));
    }
    let held = members
        .records("membership")?
        .iter()
        .any(|(_, m)| listed(m, "roles", "role").iter().any(|r| r == role));
    if held {
        return Err(bad(
            "RoleInUse",
            format!("someone holds {role}: give them another first"),
        ));
    }
    let granted: Vec<Grant> = members
        .grants
        .iter()
        .filter(|g| g.role != role)
        .cloned()
        .collect();
    let defined = members.records("role")?.iter().any(|(k, _)| k == role);
    if defined {
        let gone = Write::Delete {
            collection: format!("{NS}.role"),
            rkey: role.into(),
        };
        host.space_write(&members.group, &members.space, &[gone])
            .await?;
    }
    let permissions = members.permissions(&granted);
    let space = members.space.clone();
    let permissions_collection = format!("{NS}.permissions");
    let put = put_in(
        &host,
        &members.group,
        &space,
        &permissions_collection,
        "self",
        permissions,
    );
    let _written = put.await?;
    Ok(Json(json!({})))
}

/// Gives a member the roles named, in place of theirs: each one added or
/// taken away one the caller may give. Everyone in a group holds `member`,
/// which goes only when they do.
pub async fn assign_roles(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, _, mine) =
        acting(&host, &headers, &body, "assignRoles", Some("role.assign")).await?;
    let member = body["member"]
        .as_str()
        .ok_or_else(|| invalid("member is required"))?;
    if body["roles"].as_array().is_none() {
        return Err(invalid("roles is required"));
    }
    let roles = listed(&body, "roles", "role");
    let current = members
        .membership(member)?
        .ok_or_else(|| bad("NotAMember", format!("{member} is not a member")))?;
    if current.iter().any(|r| r == "member") && !roles.iter().any(|r| r == "member") {
        return Err(bad(
            "MemberRole",
            "everyone in the group holds member: to put someone out, eject them",
        ));
    }
    let defined: BTreeSet<String> = members
        .records("role")?
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    let undefined: Vec<&String> = roles.iter().filter(|r| !defined.contains(*r)).collect();
    if !undefined.is_empty() {
        return Err(invalid(format!("no such roles: {undefined:?}")));
    }
    let assignable = members.assignable(&mine, "role.assign");
    let changed = roles
        .iter()
        .filter(|r| !current.contains(r))
        .chain(current.iter().filter(|r| !roles.contains(r)));
    let blocked: Vec<&String> = changed.filter(|r| !assignable.contains(*r)).collect();
    if !blocked.is_empty() {
        return Err(bad(
            "NotAssignable",
            format!("not yours to give or take: {blocked:?}"),
        ));
    }
    members.keeps_a_configurer(member, &roles)?;
    members.put_membership(member, &roles).await?;
    Ok(Json(json!({})))
}

impl Host {
    /// Takes `member` out of `group`: their membership goes, their
    /// credentials are revoked, and the index forgets what they wrote.
    async fn put_out(&self, group: &str, member: &str) -> Result<(), XrpcError> {
        // Their credentials first: if that fails, they are still in, and
        // it may be asked again.
        let revoked = self.revoke_credentials(group, member).await;
        revoked.map_err(XrpcError::internal)?;
        // And the apps they signed in as the group.
        let ended = self.store.revoke_member_grants(group, member);
        ended.map_err(XrpcError::internal)?;
        let dropped = self.store.drop_members_app_passwords(group, member);
        dropped.map_err(XrpcError::internal)?;
        let gone = Write::Delete {
            collection: format!("{NS}.membership"),
            rkey: did_rkey(member),
        };
        let (space, gone) = (members_space(group), [gone]);
        let written = self.space_write(group, &space, &gone);
        written.await?;
        for space in self.store.spaces_of(group).map_err(XrpcError::internal)? {
            self.store
                .unindex(&space, member)
                .map_err(XrpcError::internal)?;
        }
        Ok(())
    }
}

/// Puts a member out of the group, for one holding `eject` whose roles may
/// give or take each of theirs.
pub async fn eject_member(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, _, mine) = acting(&host, &headers, &body, "ejectMember", Some("eject")).await?;
    let member = body["member"]
        .as_str()
        .ok_or_else(|| invalid("member is required"))?;
    let theirs = members
        .membership(member)?
        .ok_or_else(|| bad("NotAMember", format!("{member} is not a member")))?;
    let assignable = members.assignable(&mine, "eject");
    let blocked: Vec<&String> = theirs.iter().filter(|r| !assignable.contains(*r)).collect();
    if !blocked.is_empty() {
        return Err(bad(
            "NotAssignable",
            format!("cannot put out a holder of {blocked:?}"),
        ));
    }
    members.keeps_a_configurer(member, &[])?;
    host.put_out(&members.group, member).await?;
    Ok(Json(json!({})))
}

/// Leaves the group. Its last member able to configure it may not: nobody
/// could run it again.
pub async fn leave_group(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, me, _) = acting(&host, &headers, &body, "leaveGroup", None).await?;
    if members.membership(&me)?.is_none() {
        return Err(bad("NotAMember", "you are not a member"));
    }
    members.keeps_a_configurer(&me, &[])?;
    host.put_out(&members.group, &me).await?;
    Ok(Json(json!({})))
}
