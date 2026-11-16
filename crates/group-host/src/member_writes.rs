//! The members space's records, written as the group with a token: its
//! roles, what each may do (the permissions record), and who holds which
//! (memberships). The host makes each write only as it keeps the group
//! whole, and does what the write means.
//!
//! - A role is a key of 64 characters at most. It goes only once nobody
//!   holds it and the permissions record grants it nothing.
//! - The permissions record is the configurers'. In it a writer changes
//!   the grants only of roles they may assign, adds no scope their own
//!   roles do not hold, and lets no role assign one they may not. A new
//!   role, which nobody holds and no grant names, is theirs to grant and
//!   to list.
//! - Writing a membership for someone waiting to join admits them, with
//!   roles the writer's `admit` grants may give, and their request goes.
//!   Writing one for a member changes their roles, each one added or taken
//!   away one the writer's `role.assign` grants may give. Everyone in the
//!   group holds `member`.
//! - Deleting a membership puts the member out, for one whose `eject`
//!   grants may give each of their roles: their credentials are revoked,
//!   the apps they signed in as the group end, their app passwords go,
//!   they leave the writer set of each of its spaces (the index forgets
//!   what they wrote, and the spaces' syncers are told), and the ejection
//!   waits in the group's moderation queue.
//! - Whatever is written, someone is left holding a role that configures
//!   the group, if anyone did.
//!
//! Each write of a batch is asked of the records as the writes before it
//! left them, and the writer's roles as they were before any.

use crate::Host;
use crate::api::invalid;
use crate::group::did_rkey;
use crate::group_members::Members;
use crate::moderation::Event;
use crate::oauth_resource::Acting;
use crate::oauth_scope::{scopes_of, set_of};
use crate::rbac::{Grant, NS, actions_for, actions_of, roles_of};
use crate::repo::Write;
use crate::scopes::covers;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use axum::http::StatusCode;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// The longest role key.
const ROLE_KEY_AT_MOST: usize = 64;

/// Which of the members space's collections a write is of, if one of
/// these.
fn kind(write: &Write) -> Option<&'static str> {
    let (collection, _) = write.path();
    let name = collection.strip_prefix(NS)?.strip_prefix('.')?;
    ["role", "permissions", "membership"]
        .into_iter()
        .find(|k| *k == name)
}

/// What a holder of `roles` may write in the members space, as the group,
/// by collection: roles and the permissions record if they configure the
/// group; memberships as they may admit (create), give roles (update) and
/// put out (delete).
pub fn members_writes(grants: &[Grant], roles: &[String]) -> BTreeMap<String, BTreeSet<String>> {
    let actions = actions_for(grants, roles);
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if actions.contains("group.configure") {
        for c in ["role", "permissions"] {
            let all = ["create", "update", "delete"].map(String::from);
            out.insert(format!("{NS}.{c}"), all.into());
        }
    }
    let by = [
        ("admit", "create"),
        ("role.assign", "update"),
        ("eject", "delete"),
    ];
    let memberships: BTreeSet<String> = by
        .iter()
        .filter(|(a, _)| actions.contains(a))
        .map(|(_, w)| w.to_string())
        .collect();
    if !memberships.is_empty() {
        out.insert(format!("{NS}.membership"), memberships);
    }
    out
}

/// The space scopes that write, in `space`, what `writes` lists, by
/// collection: none in the group's public repo, where these records are
/// not kept.
pub(crate) fn own_scopes(
    space: &SpaceRef,
    writes: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let at = format!(
        "space:{}?authority={}&skey={}",
        space.kind, space.authority, space.skey
    );
    let scope = |(c, actions): (&String, &BTreeSet<String>)| {
        let actions: Vec<String> = actions.iter().map(|a| format!("&action={a}")).collect();
        format!("{at}&collection={c}&action=read{}", actions.concat())
    };
    writes.iter().map(scope).collect()
}

/// The roles holders of `roles` may give or take, for `action`, or for
/// any if none is named.
fn assignable(grants: &[Grant], roles: &[String], action: Option<&str>) -> BTreeSet<String> {
    let theirs = grants.iter().filter(|g| roles.contains(&g.role));
    let doing = theirs.filter(|g| action.is_none_or(|a| actions_of(&g.scopes).contains(a)));
    doing.flat_map(|g| g.assignable.iter().cloned()).collect()
}

/// The scopes `scope` is, a permission set of ours as those it lists.
fn expanded(scope: &str) -> Vec<String> {
    match set_of(scope) {
        Some(set) => scopes_of(set),
        None => vec![scope.to_string()],
    }
}

/// The members space as the writes of a batch leave it.
#[derive(Clone)]
struct Book {
    roles: BTreeSet<String>,
    grants: Vec<Grant>,
    /// Each membership by its key: whose it is, and their roles.
    members: BTreeMap<String, (String, Vec<String>)>,
}

impl Book {
    fn of(members: &Members) -> Result<Book, XrpcError> {
        let roles = members.records("role")?.into_iter().map(|(k, _)| k);
        let held = members.records("membership")?.into_iter().map(|(k, m)| {
            let did = m["member"].as_str().unwrap_or_default().to_string();
            (k, (did, roles_of(&m)))
        });
        Ok(Book {
            roles: roles.collect(),
            grants: members.grants.clone(),
            members: held.collect(),
        })
    }

    /// Whether anyone holds a role that configures the group.
    fn run(&self) -> bool {
        let configuring: BTreeSet<&String> = self
            .grants
            .iter()
            .filter(|g| actions_of(&g.scopes).contains("group.configure"))
            .map(|g| &g.role)
            .collect();
        let holds = |roles: &Vec<String>| roles.iter().any(|r| configuring.contains(r));
        self.members.values().any(|(_, roles)| holds(roles))
    }

    /// The roles defined that nobody holds and no grant names: new, and
    /// so anyone's who configures the group to grant and to give.
    fn fresh(&self) -> BTreeSet<String> {
        let named = |r: &String| {
            let granted = self
                .grants
                .iter()
                .any(|g| &g.role == r || g.assignable.contains(r));
            granted || self.members.values().any(|(_, roles)| roles.contains(r))
        };
        self.roles.iter().filter(|r| !named(r)).cloned().collect()
    }

    /// Refuses `roles` unless each is defined.
    fn defined<'a>(&self, roles: impl IntoIterator<Item = &'a String>) -> Result<(), XrpcError> {
        let undefined: Vec<&String> = roles
            .into_iter()
            .filter(|r| !self.roles.contains(*r))
            .collect();
        if !undefined.is_empty() {
            return Err(invalid(format!("no such roles: {undefined:?}")));
        }
        Ok(())
    }
}

/// Who writes, as the group: their roles and what the group grants them,
/// as they were before the writes.
struct Writer<'a> {
    group: &'a str,
    roles: &'a [String],
    grants: &'a [Grant],
}

impl Writer<'_> {
    /// Refuses a membership whose key is not its member's DID, or which
    /// leaves out `member`.
    fn membership(&self, rkey: &str, value: &Value) -> Result<(String, Vec<String>), XrpcError> {
        let did = value["member"].as_str().unwrap_or_default().to_string();
        if did_rkey(&did) != rkey {
            return Err(invalid("a membership is kept at its member's DID"));
        }
        let roles = roles_of(value);
        if !roles.iter().any(|r| r == "member") {
            return Err(bad(
                "MemberRole",
                "everyone in the group holds member: to put someone out, delete their membership",
            ));
        }
        Ok((did, roles))
    }

    /// Refuses `changed` unless the writer's grants for `action` may give
    /// or take each.
    fn may_give<'a>(
        &self,
        changed: impl IntoIterator<Item = &'a String>,
        action: &str,
    ) -> Result<(), XrpcError> {
        let mine = assignable(self.grants, self.roles, Some(action));
        let blocked: Vec<&String> = changed.into_iter().filter(|r| !mine.contains(*r)).collect();
        if !blocked.is_empty() {
            return Err(bad(
                "NotAssignable",
                format!("not yours to give or take: {blocked:?}"),
            ));
        }
        Ok(())
    }

    /// Refuses a new permissions record, in place of `was`, that changes a
    /// grant of a role the writer may not assign, adds a scope their roles
    /// do not hold, or lets a role assign one they may not.
    fn permissions(
        &self,
        was: &[Grant],
        now: &[Grant],
        fresh: &BTreeSet<String>,
    ) -> Result<Vec<String>, XrpcError> {
        let mut seen = BTreeSet::new();
        if let Some(twice) = now.iter().find(|g| !seen.insert(&g.role)) {
            return Err(invalid(format!(
                "one grant for each role: {} has two",
                twice.role
            )));
        }
        let union = assignable(self.grants, self.roles, None);
        let mine = |r: &String| union.contains(r) || fresh.contains(r);
        let held: Vec<String> = self
            .grants
            .iter()
            .filter(|g| self.roles.contains(&g.role))
            .flat_map(|g| g.scopes.iter().flat_map(|s| expanded(s)))
            .collect();
        let find = |grants: &[Grant], role: &str| grants.iter().find(|g| g.role == role).cloned();
        let roles: BTreeSet<&String> = was.iter().chain(now).map(|g| &g.role).collect();
        let mut named = Vec::new();
        for role in roles {
            let (before, after) = (find(was, role), find(now, role));
            if before == after {
                continue;
            }
            if !mine(role) {
                return Err(bad(
                    "NotAssignable",
                    format!("not yours to change: {role}'s grant"),
                ));
            }
            let Some(after) = after else { continue };
            let before = before.unwrap_or(Grant {
                role: role.clone(),
                scopes: Vec::new(),
                assignable: Vec::new(),
            });
            let added = after.scopes.iter().filter(|s| !before.scopes.contains(s));
            for scope in added {
                let covered = expanded(scope).iter().all(|s| covers(&held, s, self.group));
                if !covered {
                    return Err(bad(
                        "ScopeNotHeld",
                        format!("your roles do not hold {scope}, to give it"),
                    ));
                }
            }
            let listed = after
                .assignable
                .iter()
                .filter(|r| !before.assignable.contains(r));
            let listed: Vec<&String> = listed.collect();
            if let Some(r) = listed.iter().find(|r| !mine(r)) {
                return Err(bad("NotAssignable", format!("not yours to give: {r}")));
            }
            named.push(role.clone());
            named.extend(listed.into_iter().cloned());
        }
        Ok(named)
    }
}

impl Host {
    /// What a holder of `roles` may write in `group`'s members space, as
    /// the group, by [`members_writes`].
    pub(crate) fn members_writes_of(
        &self,
        group: &str,
        roles: &[String],
    ) -> Result<BTreeMap<String, BTreeSet<String>>, XrpcError> {
        let members = Members::of(self, group)?;
        Ok(members_writes(&members.grants, roles))
    }

    /// Refuses `writes` in `group`'s members space, by the member who signed
    /// the app in (holding `roles`, under `grants`), that the records'
    /// rules do not allow.
    pub(crate) fn members_rules(
        &self,
        group: &str,
        roles: &[String],
        grants: &[Grant],
        writes: &[Write],
    ) -> Result<(), XrpcError> {
        let members = Members::of(self, group)?;
        let before = Book::of(&members)?;
        let mut book = before.clone();
        let mut named = Vec::new();
        let writer = Writer {
            group,
            roles,
            grants,
        };
        // A label is held to the rules of every space (`moderation`).
        for w in writes.iter().filter(|w| kind(w).is_some()) {
            let (_, rkey) = w.path();
            match (kind(w), w) {
                (Some("role"), Write::Delete { .. }) => {
                    book.roles.remove(rkey);
                }
                (Some("role"), _) => {
                    if rkey.len() > ROLE_KEY_AT_MOST {
                        return Err(invalid(format!(
                            "a role is a key of {ROLE_KEY_AT_MOST} characters at most"
                        )));
                    }
                    book.roles.insert(rkey.to_string());
                }
                (Some("permissions"), Write::Delete { .. }) => {
                    return Err(invalid("the group keeps its permissions"));
                }
                (
                    Some("permissions"),
                    Write::Create { value, .. } | Write::Update { value, .. },
                ) => {
                    if rkey != "self" {
                        return Err(invalid("the permissions record is kept at self"));
                    }
                    let now = crate::rbac::grants(value);
                    let names = writer.permissions(&book.grants, &now, &book.fresh())?;
                    named.extend(names);
                    book.grants = now;
                }
                // The rest are memberships: the members space takes no other
                // record of ours (`guard`).
                (_, Write::Delete { .. }) => {
                    let (_, theirs) = book.members.remove(rkey).unwrap_or_default();
                    writer.may_give(&theirs, "eject")?;
                }
                (_, Write::Create { value, .. }) => {
                    let (did, roles) = writer.membership(rkey, value)?;
                    let asked = self.store.asked(group, &did).map_err(XrpcError::internal)?;
                    if asked.is_none() {
                        return Err(bad(
                            "JoinRequestNotFound",
                            format!("{did} has not asked to join: a membership admits one who has"),
                        ));
                    }
                    book.defined(&roles)?;
                    writer.may_give(&roles, "admit")?;
                    book.members.insert(rkey.to_string(), (did, roles));
                }
                (_, Write::Update { value, .. }) => {
                    let (did, roles) = writer.membership(rkey, value)?;
                    book.defined(&roles)?;
                    let (_, current) = book.members.get(rkey).cloned().unwrap_or_default();
                    let added = roles.iter().filter(|r| !current.contains(r));
                    let taken = current.iter().filter(|r| !roles.contains(r));
                    writer.may_give(added.chain(taken), "role.assign")?;
                    book.members.insert(rkey.to_string(), (did, roles));
                }
            }
        }
        // A role goes only once nothing holds or grants it.
        for gone in before.roles.difference(&book.roles) {
            let held = book.members.values().any(|(_, r)| r.contains(gone));
            let granted = book.grants.iter().any(|g| &g.role == gone);
            if held || granted {
                let what = if held {
                    "someone holds it"
                } else {
                    "it still has a grant"
                };
                return Err(bad("RoleInUse", format!("{gone} stays while {what}")));
            }
        }
        // A grant changed is of a role defined, and gives roles defined.
        book.defined(&named)?;
        if before.run() && !book.run() {
            return Err(bad(
                "LastConfigurer",
                "nobody would be left who can run this group: give that to someone else first",
            ));
        }
        Ok(())
    }

    /// The members whose memberships `writes` delete, by DID, read before
    /// they go; and the space credentials of each revoked first, if the
    /// deletes may be made, so that if revoking fails they are still in
    /// and the write may be sent again.
    pub(crate) async fn leaving(
        &self,
        acting: &Acting,
        uri: &str,
        space: &SpaceRef,
        writes: &[Write],
    ) -> Result<BTreeMap<String, String>, XrpcError> {
        let group = &acting.grant.group;
        let mut out = BTreeMap::new();
        let mut gone = Vec::new();
        let deletes = writes.iter().filter(|w| matches!(w, Write::Delete { .. }));
        for w in deletes.filter(|w| kind(w) == Some("membership")) {
            let (collection, rkey) = w.path();
            let record = self.group_record(group, uri, collection, rkey);
            let Some(record) = record.map_err(XrpcError::internal)? else {
                continue;
            };
            let did = record["member"].as_str().unwrap_or(rkey);
            out.insert(rkey.to_string(), did.to_string());
            gone.push(w.clone());
        }
        // Asked of the deletes alone: the write asks again of them all.
        if !gone.is_empty() && self.may_space(acting, uri, space, &gone).is_ok() {
            for member in out.values() {
                let revoked = self.revoke_credentials(group, member).await;
                revoked.map_err(XrpcError::internal)?;
            }
        }
        Ok(out)
    }

    /// Does what the members space's `made` writes mean, by `acting`'s
    /// member: a request decided for each membership written, and each
    /// one deleted, its member, by `leaving`, put out.
    pub(crate) async fn members_written(
        &self,
        acting: &Acting,
        made: &[Write],
        leaving: &BTreeMap<String, String>,
    ) -> Result<(), XrpcError> {
        let (group, by) = (&acting.grant.group, &acting.grant.member);
        for w in made.iter().filter(|w| kind(w) == Some("membership")) {
            match w {
                Write::Delete { rkey, .. } => {
                    let member = leaving.get(rkey).map_or(rkey.as_str(), String::as_str);
                    self.cut_off(group, member).await?;
                    self.remove_writer_of(group, member)?;
                    // Kept in the group's queue, on them, for its moderators.
                    let event = Event {
                        kind: "eject".into(),
                        actor: Some(by.clone()),
                        created_at: now_millis(),
                        ..Event::default()
                    };
                    let noted = self.store.moderate(group, member, &event);
                    noted.map_err(XrpcError::internal)?;
                }
                Write::Create { value, .. } | Write::Update { value, .. } => {
                    let did = value["member"].as_str().unwrap_or_default();
                    let decided = self.store.decide(group, did);
                    decided.map_err(XrpcError::internal)?;
                }
            }
        }
        Ok(())
    }
}
