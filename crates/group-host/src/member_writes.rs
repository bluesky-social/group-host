//! The members space's records, written as the group with a token: its
//! roles, each with what it may do across the group, and who holds which
//! (memberships). The host makes each write only as it keeps the group
//! whole, and does what the write means.
//!
//! - A role is a key of 64 characters at most. It goes only once nobody
//!   holds it.
//! - Roles are the configurers'. In a role a writer changes the grant
//!   (its scopes and `assignable`) only of a role they may assign, or one
//!   the batch makes; adds no scope their own roles do not hold, a
//!   permission set as the scopes it lists; and lets the role assign only
//!   roles they may assign. `*` in `assignable` is every role, those made
//!   later too, and only one who may assign every role may give it.
//! - Writing a membership for someone waiting to join admits them, with
//!   roles the writer's `admit` grants may give, and their request goes.
//!   Writing one for a member changes their roles, each one added or taken
//!   away one the writer's `role.assign` grants may give. Everyone in the
//!   group holds `member`. Whoever's roles a write changes leaves the
//!   writer set of each space whose policy no longer grants them `write`.
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
use crate::group::{did_rkey, rkey_did};
use crate::group_members::Members;
use crate::moderation::Event;
use crate::oauth_resource::Acting;
use crate::oauth_scope::{scopes_of, set_of};
use crate::rbac::{EVERY_ROLE, Grant, NS, actions_for, actions_of, assigns, roles_of};
use crate::repo::Write;
use crate::scopes::covers;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use atproto::syntax::is_did;
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
    ["role", "membership"].into_iter().find(|k| *k == name)
}

/// What a holder of `roles` may write in the members space, as the group,
/// by collection: roles if they configure the group; memberships as they
/// may admit (create), give roles (update) and put out (delete).
pub fn members_writes(grants: &[Grant], roles: &[String]) -> BTreeMap<String, BTreeSet<String>> {
    let actions = actions_for(grants, roles);
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if actions.contains("group.configure") {
        let all = ["create", "update", "delete"].map(String::from);
        out.insert(format!("{NS}.role"), all.into());
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
/// any if none is named: `*` among them for every role.
pub(crate) fn assignable(
    grants: &[Grant],
    roles: &[String],
    action: Option<&str>,
) -> BTreeSet<String> {
    let theirs = grants.iter().filter(|g| roles.contains(&g.role));
    let doing = theirs.filter(|g| action.is_none_or(|a| actions_of(&g.scopes).contains(a)));
    doing.flat_map(|g| g.assignable.iter().cloned()).collect()
}

/// The scopes `scope` is, a permission set of ours as those it lists.
pub(crate) fn expanded(scope: &str) -> Vec<String> {
    match set_of(scope) {
        Some(set) => scopes_of(set),
        None => vec![scope.to_string()],
    }
}

/// The scopes holders of `roles` hold across the group, each permission
/// set as the scopes it lists.
pub(crate) fn held(grants: &[Grant], roles: &[String]) -> Vec<String> {
    let theirs = grants.iter().filter(|g| roles.contains(&g.role));
    theirs
        .flat_map(|g| g.scopes.iter().flat_map(|s| expanded(s)))
        .collect()
}

/// The members space as the writes of a batch leave it.
#[derive(Clone)]
struct Book {
    /// Each role by its key, and its grant.
    roles: BTreeMap<String, Grant>,
    /// Each membership by its key, its member's DID: their roles.
    members: BTreeMap<String, Vec<String>>,
}

impl Book {
    fn of(members: &Members) -> Result<Book, XrpcError> {
        let roles = members.grants.iter().map(|g| (g.role.clone(), g.clone()));
        let held = members.records("membership")?.into_iter();
        Ok(Book {
            roles: roles.collect(),
            members: held.map(|(k, m)| (k, roles_of(&m))).collect(),
        })
    }

    /// Whether anyone holds a role that configures the group.
    fn run(&self) -> bool {
        let configuring = |r: &String| {
            let grant = self.roles.get(r);
            grant.is_some_and(|g| actions_of(&g.scopes).contains("group.configure"))
        };
        let mut held = self.members.values().flatten();
        held.any(configuring)
    }

    /// Refuses `roles` unless each is defined.
    fn defined<'a>(&self, roles: impl IntoIterator<Item = &'a String>) -> Result<(), XrpcError> {
        let undefined: Vec<&String> = roles
            .into_iter()
            .filter(|r| !self.roles.contains_key(*r))
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
    /// leaves out `member`: their DID, and their roles.
    fn membership(&self, rkey: &str, value: &Value) -> Result<(String, Vec<String>), XrpcError> {
        let did = rkey_did(rkey);
        if !is_did(&did) || did_rkey(&did) != rkey {
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
        let blocked: Vec<&String> = changed.into_iter().filter(|r| !assigns(&mine, r)).collect();
        if !blocked.is_empty() {
            return Err(bad(
                "NotAssignable",
                format!("not yours to give or take: {blocked:?}"),
            ));
        }
        Ok(())
    }

    /// Refuses a write of the role `key`, its grant `was` before and `now`
    /// after (none for none, as one made or deleted), that changes the
    /// grant of a role the writer may not assign unless the batch makes
    /// it (`new`), adds a scope their roles do not hold, or lets the role
    /// assign one they may not. The roles it lets the role assign anew.
    fn role(
        &self,
        key: &str,
        was: Option<&Grant>,
        now: Option<&Grant>,
        new: bool,
    ) -> Result<Vec<String>, XrpcError> {
        let none = Grant::of(key, &Value::Null);
        let (before, after) = (was.unwrap_or(&none), now.unwrap_or(&none));
        if (&before.scopes, &before.assignable) == (&after.scopes, &after.assignable) {
            return Ok(Vec::new());
        }
        let union = assignable(self.grants, self.roles, None);
        if !new && !assigns(&union, key) {
            return Err(bad(
                "NotAssignable",
                format!("not yours to change: {key}'s grant"),
            ));
        }
        let held = held(self.grants, self.roles);
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
        let listed = after.assignable.iter();
        let listed: Vec<&String> = listed.filter(|r| !before.assignable.contains(r)).collect();
        // Every role only from one who may give every role.
        let mine = |r: &&String| match r.as_str() {
            EVERY_ROLE => union.contains(EVERY_ROLE),
            _ => assigns(&union, r),
        };
        if let Some(r) = listed.iter().find(|r| !mine(r)) {
            return Err(bad("NotAssignable", format!("not yours to give: {r}")));
        }
        let named = listed.into_iter().filter(|r| *r != EVERY_ROLE);
        Ok(named.cloned().collect())
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
                    let was = book.roles.remove(rkey);
                    let new = !before.roles.contains_key(rkey);
                    writer.role(rkey, was.as_ref(), None, new)?;
                }
                (Some("role"), Write::Create { value, .. } | Write::Update { value, .. }) => {
                    if rkey.len() > ROLE_KEY_AT_MOST {
                        return Err(invalid(format!(
                            "a role is a key of {ROLE_KEY_AT_MOST} characters at most"
                        )));
                    }
                    let now = Grant::of(rkey, value);
                    let new = !before.roles.contains_key(rkey);
                    let names = writer.role(rkey, book.roles.get(rkey), Some(&now), new)?;
                    named.extend(names);
                    book.roles.insert(rkey.to_string(), now);
                }
                // The rest are memberships: the members space takes no other
                // record of ours (`guard`).
                (_, Write::Delete { .. }) => {
                    let theirs = book.members.remove(rkey).unwrap_or_default();
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
                    book.members.insert(rkey.to_string(), roles);
                }
                (_, Write::Update { value, .. }) => {
                    let (_, roles) = writer.membership(rkey, value)?;
                    book.defined(&roles)?;
                    let current = book.members.get(rkey).cloned().unwrap_or_default();
                    let added = roles.iter().filter(|r| !current.contains(r));
                    let taken = current.iter().filter(|r| !roles.contains(r));
                    writer.may_give(added.chain(taken), "role.assign")?;
                    book.members.insert(rkey.to_string(), roles);
                }
            }
        }
        // A role goes only once nobody holds it.
        for gone in before.roles.keys().filter(|r| !book.roles.contains_key(*r)) {
            if book.members.values().any(|r| r.contains(gone)) {
                return Err(bad(
                    "RoleInUse",
                    format!("{gone} stays while someone holds it"),
                ));
            }
        }
        // A role is let assign only roles defined.
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
            if record.map_err(XrpcError::internal)?.is_none() {
                continue;
            }
            out.insert(rkey.to_string(), rkey_did(rkey));
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
                Write::Create { rkey, .. } | Write::Update { rkey, .. } => {
                    let did = rkey_did(rkey);
                    let decided = self.store.decide(group, &did);
                    decided.map_err(XrpcError::internal)?;
                    // Roles taken away may take writing in a space with them.
                    let pruned = self.prune_writer_of(group, &did);
                    pruned.map_err(XrpcError::internal)?;
                }
            }
        }
        Ok(())
    }
}
