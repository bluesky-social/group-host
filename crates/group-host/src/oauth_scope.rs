//! What a token signing in as a group may do: the scope an app asks for,
//! narrowed to what the approving member's roles let them do as the
//! group, then made concrete, so the token never depends on how a scope
//! is read later.
//!
//! The permission sets a token keeps are this host's own for a group's
//! session (`org.atgrp.authManageBasic`, `authManageMembers` and
//! `authModerate`, and `authManageAdmin`, which holds all three), read
//! from their lexicons here, and other apps' sets, read as they publish
//! them (`permission_sets`): each an upper bound, of which the parts the
//! roles grant are kept. A space scope naming no collection keeps every
//! one, which narrowing allowed only to roles that may write every one
//! there.

use crate::Host;
use crate::group::{members_space, meta_space};
use crate::lexicons::lexicons;
use crate::permission_sets::{Sets, included};
use crate::rbac::{self, LABEL, NS, actions_for, label_writes, meta_writes, repo_collections_for};
use crate::scopes::blob_scopes;
use crate::xrpc::XrpcError;
use atproto::scope::{Rpc, Scope, SpaceAction};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub use crate::rbac::WRITES;

/// This host's permission sets for a group's session, by name: the last
/// holds the other three.
pub const TIERS: [&str; 4] = [
    "authManageBasic",
    "authManageMembers",
    "authModerate",
    "authManageAdmin",
];

/// The action each of the host's methods a permission set lists asks of
/// the member who signed the app in.
const ASKS: [(&str, &str); 7] = [
    ("org.atgrp.listJoinRequests", "admit"),
    ("org.atgrp.denyJoinRequest", "admit"),
    ("org.atgrp.internal.listInvites", "invite"),
    ("org.atgrp.mod.querySubjects", "mod.read"),
    ("org.atgrp.mod.querySubjectHistory", "mod.read"),
    ("org.atgrp.internal.getModSubjectBlob", "mod.read"),
    ("org.atgrp.mod.updateSubject", "mod.resolve"),
];

/// The permission set of this host's that `scope` includes, from its
/// lexicon, if it is one: `include:org.atgrp.authModerate`.
pub fn set_of(scope: &str) -> Option<&'static Value> {
    let name = scope.strip_prefix(&format!("include:{NS}."))?;
    let set = lexicons().def(&format!("{NS}.{name}"));
    set.filter(|d| d["type"] == "permission-set")
}

/// The permission set `scope` includes, if it is one of [`TIERS`].
fn tier_of(scope: &str) -> Option<&'static Value> {
    let name = scope.strip_prefix(&format!("include:{NS}."))?;
    TIERS.contains(&name).then(|| set_of(scope)).flatten()
}

/// The parts of a `space:` scope that decide whether a role may have it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceScope {
    pub kind: String,
    pub authority: String,
    pub skey: String,
    pub collections: Vec<String>,
    /// The writes asked for: every one, when it names none.
    pub writes: Vec<String>,
    /// Whether it asks only to read.
    pub read_only: bool,
    /// What it asks to do to the spaces themselves: `create`, `update`,
    /// `delete`.
    pub manage: Vec<String>,
}

impl SpaceScope {
    /// `scope`, if it is a space scope, as atproto reads one.
    pub fn parse(scope: &str) -> Option<SpaceScope> {
        let Scope::Space(s) = Scope::parse(scope) else {
            return None;
        };
        let asked = |w: &&str| s.actions.iter().any(|a| a.as_str() == *w);
        let named: Vec<String> = WRITES.into_iter().filter(asked).map(String::from).collect();
        let writes = if named.is_empty() {
            WRITES.map(String::from).to_vec()
        } else {
            named
        };
        let manage: Vec<String> = s
            .manage
            .iter()
            .flatten()
            .map(|m| m.as_str().into())
            .collect();
        let reads = |a: &SpaceAction| matches!(a, SpaceAction::Read | SpaceAction::ReadSelf);
        Some(SpaceScope {
            read_only: manage.is_empty() && s.actions.iter().all(reads),
            kind: s.space_type,
            authority: s.authority,
            skey: s.skey,
            collections: s.collections.unwrap_or_default(),
            writes,
            manage,
        })
    }

    /// Whether it may be for `group`'s spaces.
    fn for_group(&self, group: &str) -> bool {
        ["*", "self", group].contains(&self.authority.as_str())
    }
}

/// The collections `scope` names, if it is a repo scope.
fn repo_collections(scope: &str) -> Option<Vec<String>> {
    match Scope::parse(scope) {
        Scope::Repo(r) => Some(r.collections),
        _ => None,
    }
}

/// Whether a holder of `actions`, with `rooms`, may have the scope `want`,
/// which manages spaces: each thing it asks to do to spaces of its type
/// they may do to any of the group's spaces, or, for one of a type named,
/// by the policy of one of that type it names.
fn manages(want: &SpaceScope, actions: &BTreeSet<&'static str>, rooms: &[Room]) -> bool {
    let across = managing(actions);
    let named =
        |room: &&Room| room.kind == want.kind && (want.skey == "*" || room.skey == want.skey);
    want.manage.iter().all(|op| {
        let by_policy = |room: &Room| room.manages.contains(op.as_str());
        across.contains(op.as_str()) || rooms.iter().filter(named).any(by_policy)
    })
}

/// `writable` with the label writes `labels`, if it does not hold each of
/// them already.
pub fn with_labels(writable: &mut BTreeMap<String, BTreeSet<String>>, labels: BTreeSet<String>) {
    if !labels.iter().all(|w| may_write(writable, LABEL, w)) {
        writable.entry(LABEL.into()).or_default().extend(labels);
    }
}

/// Whether those writes include `action` on `collection`.
pub(crate) fn may_write(
    writes: &BTreeMap<String, BTreeSet<String>>,
    collection: &str,
    action: &str,
) -> bool {
    ["*", collection]
        .iter()
        .any(|c| writes.get(*c).is_some_and(|w| w.contains(action)))
}

/// Whether a kind of record and a kind of space are one app's: the same
/// first two segments, as `org.atgrp`.
fn same_app(a: &str, b: &str) -> bool {
    let app = |n: &str| n.split('.').take(2).collect::<Vec<_>>().join(".");
    app(a) == app(b)
}

/// One of the group's spaces, and what the roles may do in it.
struct Room {
    kind: String,
    skey: String,
    readable: bool,
    writable: BTreeMap<String, BTreeSet<String>>,
    /// What they may do to the space itself (`update`, `delete`), by
    /// their roles across the group or its policy's grants.
    manages: BTreeSet<&'static str>,
}

/// What `actions` let their holder do to any of the group's spaces itself.
fn managing(actions: &BTreeSet<&'static str>) -> BTreeSet<&'static str> {
    let by = [
        ("space.create", "create"),
        ("space.configure", "update"),
        ("space.delete", "delete"),
    ];
    by.iter()
        .filter(|(a, _)| actions.contains(a))
        .map(|(_, op)| *op)
        .collect()
}

impl Host {
    /// The group's spaces, each with what a holder of `roles`, holding
    /// `actions` by `grants`, may read and write there as the group, and
    /// do to it.
    fn rooms(
        &self,
        group: &str,
        roles: &[String],
        grants: &[rbac::Grant],
        actions: &BTreeSet<&'static str>,
    ) -> Result<Vec<Room>, XrpcError> {
        let spaces = self.store.spaces_of(group).map_err(XrpcError::internal)?;
        let mut rooms = Vec::new();
        for uri in spaces {
            let policy = self.policy(&uri).map_err(XrpcError::internal)?;
            let policy = policy.expect("a space the group has");
            let mut writable = policy.writes(roles);
            // The meta and members spaces' own records, as the group's
            // roles grant them.
            if uri == meta_space(group) {
                writable.extend(meta_writes(grants, roles, group));
            }
            if uri == members_space(group) {
                writable.extend(self.members_writes_of(group, roles)?);
            }
            with_labels(
                &mut writable,
                label_writes(grants, roles, group, &policy.space),
            );
            let mut manages = managing(actions);
            for op in ["update", "delete"] {
                if policy.manages(roles, op) {
                    manages.insert(op);
                }
            }
            rooms.push(Room {
                readable: policy.can_read(roles),
                kind: policy.space.kind,
                skey: policy.space.skey,
                writable,
                manages,
            });
        }
        Ok(rooms)
    }

    /// Whether a holder of `roles` under `grants` may write anything as
    /// `group` now: in its repo, its own spaces, or another.
    pub fn writes_anything(
        &self,
        group: &str,
        roles: &[String],
        grants: &[rbac::Grant],
    ) -> Result<bool, XrpcError> {
        let repo = repo_collections_for(grants, roles);
        let actions = actions_for(grants, roles);
        if !repo.is_empty() || actions.contains("group.configure") {
            return Ok(true);
        }
        let rooms = self.rooms(group, roles, grants, &actions)?;
        Ok(rooms.iter().any(|r| !r.writable.is_empty()))
    }

    /// Whether `rpc`'s method `method`, one of this host's own, may be
    /// given to a holder of `held`: a method a group session calls
    /// ([`ASKS`], but the `internal` ones), for this host or any
    /// audience, and the roles hold the action it asks.
    fn own_method(&self, rpc: &Rpc, method: &str, held: &BTreeSet<&'static str>) -> bool {
        let internal = format!("{NS}.internal.");
        let asks = ASKS
            .iter()
            .find(|(m, _)| *m == method && !m.starts_with(&internal));
        let ours = rpc.aud == "*" || crate::group_api::for_host(self, &rpc.aud);
        asks.is_some_and(|(_, action)| held.contains(action)) && ours
    }

    /// The scopes of `requested` a holder of `roles` in `group` may give
    /// an app acting as the group:
    ///
    /// - `atproto`, always;
    /// - one of this host's permission sets for a group's session
    ///   ([`TIERS`]), as far as the roles grant it ([`parts`]);
    /// - another app's permission set (`include:…`), read as `sets` holds
    ///   it, as far as the roles grant its parts: each collection as
    ///   `repo:C` is, and each method as `rpc:M` is; the whole set when
    ///   they grant every part, else the parts they grant;
    /// - `repo:C`, for roles whose grants hold `repo:C` or `repo:*`;
    /// - `transition:generic` and `transition:chat.bsky`, for roles with
    ///   `repo:*`, as an app password's maker holds;
    /// - `rpc:M`, a method of another service, for roles with `repo:*`;
    ///   or one of this host's own the proposal has a group session call
    ///   ([`ASKS`], but the `internal` ones), for this host or any, for
    ///   roles holding the action it asks, checked again at each call;
    /// - `space:T?…`, where some space of the group's of type `T` lets the
    ///   roles write each collection asked for, as asked, as the group; or
    ///   read it, if reading is all it asks;
    /// - `space:org.atgrp.inbox?…`, people's inboxes, for roles that
    ///   may invite, as far as it asks for invites;
    /// - `space:*?…`, made into one scope for each of the group's spaces
    ///   the roles read, writing there what they may of what it asks;
    /// - `space:T?…&manage=…`, managing spaces of type `T` (or of every
    ///   type, `*`), for roles that may do each to the group's spaces
    ///   across it, or by a space's own policy, one of type `T`;
    /// - `blob:`, beside anything above that writes.
    ///
    /// Everything else is withheld.
    pub fn narrow(
        &self,
        group: &str,
        roles: &[String],
        requested: &[String],
        sets: &Sets,
    ) -> Result<Vec<String>, XrpcError> {
        let grants = self.role_grants(group).map_err(XrpcError::internal)?;
        let repo = repo_collections_for(&grants, roles);
        let everything = repo.contains("*");
        let held = actions_for(&grants, roles);
        let rooms = self.rooms(group, roles, &grants, &held)?;
        let invite = format!("{NS}.invite");
        // What each scope asked for gives, in the order asked.
        let mut each: Vec<Vec<String>> = Vec::new();
        for s in requested {
            let scope = Scope::parse(s);
            let space = SpaceScope::parse(s).filter(|w| w.for_group(group));
            let keep = if s == "atproto" {
                true
            } else if let Some(set) = tier_of(s) {
                each.push(parts(set, &held, &rooms));
                continue;
            } else if let Some(set) = sets.get(s) {
                let all = included(s, set);
                let granted = |p: &&String| {
                    let c = repo_collections(p);
                    everything || c.is_some_and(|c| c.iter().all(|c| repo.contains(c)))
                };
                let kept: Vec<String> = all.iter().filter(granted).cloned().collect();
                each.push(match !all.is_empty() && kept == all {
                    true => vec![s.clone()],
                    false => kept,
                });
                continue;
            } else if s == "transition:generic" || s == "transition:chat.bsky" {
                everything
            } else if let Scope::Repo(r) = &scope {
                everything || r.collections.iter().all(|c| repo.contains(c))
            } else if let Scope::Rpc(r) = &scope {
                let ours = format!("{NS}.");
                r.lxms.iter().all(|m| match m.starts_with(&ours) {
                    true => self.own_method(r, m, &held),
                    false => everything && m != "*",
                })
            } else if let Some(want) = space.as_ref().filter(|w| !w.manage.is_empty()) {
                manages(want, &held, &rooms)
            } else if let Some(want) = inbox_scope(s) {
                held.contains("invite") && want.collections.iter().all(|c| *c == invite)
            } else if let Some(want) = space.as_ref().filter(|w| w.kind == "*") {
                each.push(any_kind(group, want, &rooms));
                continue;
            } else if let Some(want) = &space {
                rooms.iter().any(|room| allows(want, room))
            } else {
                false
            };
            each.push(keep.then(|| s.clone()).into_iter().collect());
        }
        let writes = |s: &String| {
            let repo_of = |set: &Value| {
                included(s, set)
                    .iter()
                    .any(|p| repo_collections(p).is_some())
            };
            tier_of(s).is_some()
                || s == "transition:generic"
                || sets.get(s).is_some_and(repo_of)
                || repo_collections(s).is_some()
                || SpaceScope::parse(s).is_some_and(|w| !w.read_only)
        };
        let any_write = each.iter().flatten().any(writes);
        let mut out: Vec<String> = Vec::new();
        for (s, mut gives) in requested.iter().zip(each) {
            if any_write && matches!(Scope::parse(s), Scope::Blob(_)) {
                gives.push(s.clone());
            }
            for g in gives {
                if !out.contains(&g) {
                    out.push(g);
                }
            }
        }
        Ok(out)
    }
}

/// Of `set`, one of [`TIERS`], what holders of `actions`, with `rooms`,
/// grant: each of its methods whose action they hold; of its space
/// permissions, what they may write of each collection it names, and do
/// to the space itself. All they grant of a set is that set, and all they
/// grant of another set within it, as a moderator's of the admin set, is
/// that other set.
fn parts(set: &Value, actions: &BTreeSet<&'static str>, rooms: &[Room]) -> Vec<String> {
    let mut out = Vec::new();
    for scope in scopes_of(set) {
        if let Scope::Rpc(rpc) = Scope::parse(&scope) {
            let asks = |l: &String| ASKS.iter().find(|(m, _)| m == l);
            let held = |l: &String| asks(l).is_some_and(|(_, a)| actions.contains(a));
            if rpc.lxms.iter().all(held) {
                out.push(scope);
            }
            continue;
        }
        let want = SpaceScope::parse(&scope).expect("a set's permissions are rpc or space");
        let whole = if !want.manage.is_empty() {
            manages(&want, actions, rooms)
        } else if want.kind == format!("{NS}.inbox") {
            actions.contains("invite")
        } else {
            let room = rooms.iter().find(|r| r.kind == want.kind);
            out.extend(room.map(|r| written(&want, r)).into_iter().flatten());
            false
        };
        if whole {
            out.push(scope);
        }
    }
    let granted: BTreeSet<&String> = out.iter().collect();
    let tier = TIERS.iter().map(|t| format!("include:{NS}.{t}")).find(|t| {
        let set = set_of(t).expect("each tier has its lexicon");
        scopes_of(set).iter().collect::<BTreeSet<_>>() == granted
    });
    match tier {
        Some(tier) => vec![tier],
        None => out,
    }
}

/// Of the space permission `want`, a set's, what `room` lets the roles
/// write of each collection it names, as one scope for each set of
/// writes, naming the collections written so.
fn written(want: &SpaceScope, room: &Room) -> Vec<String> {
    let mut by: Vec<(Vec<&String>, Vec<&String>)> = Vec::new();
    for c in &want.collections {
        let may = |w: &&String| may_write(&room.writable, c, w);
        let writes: Vec<&String> = want.writes.iter().filter(may).collect();
        match by.iter_mut().find(|(w, _)| *w == writes) {
            _ if writes.is_empty() => {}
            Some((_, collections)) => collections.push(c),
            None => by.push((writes, vec![c])),
        }
    }
    let at = format!(
        "space:{}?authority={}&skey={}",
        want.kind, want.authority, want.skey
    );
    let listed = |key: &str, values: &[&String]| -> String {
        values.iter().map(|v| format!("&{key}={v}")).collect()
    };
    by.iter()
        .map(|(writes, collections)| {
            let (c, w) = (listed("collection", collections), listed("action", writes));
            format!("{at}{c}&action=read{w}")
        })
        .collect()
}

/// `scope`, if it is for people's inboxes, where a group writes its
/// invites: any inbox, or one person's.
fn inbox_scope(scope: &str) -> Option<SpaceScope> {
    SpaceScope::parse(scope).filter(|w| w.kind == format!("{NS}.inbox"))
}

/// Whether `room` lets the roles have the space scope `want`.
fn allows(want: &SpaceScope, room: &Room) -> bool {
    if room.kind != want.kind || (want.skey != "*" && room.skey != want.skey) {
        return false;
    }
    if want.read_only {
        return room.readable;
    }
    let every = ["*".to_string()];
    let wanted = if want.collections.is_empty() {
        &every[..]
    } else {
        &want.collections
    };
    wanted
        .iter()
        .all(|c| want.writes.iter().all(|a| may_write(&room.writable, c, a)))
}

/// A scope for spaces of every type, made into one for each of the
/// group's spaces the roles read: writing what they may write there of
/// what it asks, or reading, if reading is all it asks. A grant of every
/// collection is as wide as what was asked, and of an app's records only
/// in that app's own kind of space.
fn any_kind(group: &str, want: &SpaceScope, rooms: &[Room]) -> Vec<String> {
    let asked = |c: &str| {
        want.collections.is_empty() || want.collections.iter().any(|w| w == "*" || w == c)
    };
    let mut out = Vec::new();
    for room in rooms.iter().filter(|r| r.readable) {
        if want.skey != "*" && room.skey != want.skey {
            continue;
        }
        let at = format!("space:{}?authority={group}&skey={}", room.kind, room.skey);
        let mut writes = false;
        let writable = if want.read_only {
            None
        } else {
            Some(&room.writable)
        };
        for (collection, actions) in writable.into_iter().flatten() {
            let both: Vec<&String> = want
                .writes
                .iter()
                .filter(|a| actions.contains(*a))
                .collect();
            let collections: Vec<String> = if collection != "*" {
                asked(collection)
                    .then(|| collection.clone())
                    .into_iter()
                    .collect()
            } else if asked("*") {
                vec!["*".into()]
            } else {
                let mine = want.collections.iter().filter(|c| same_app(c, &room.kind));
                mine.cloned().collect()
            };
            for c in collections.iter().filter(|_| !both.is_empty()) {
                let actions: Vec<String> = both.iter().map(|a| format!("&action={a}")).collect();
                out.push(format!(
                    "{at}&collection={c}&action=read{}",
                    actions.concat()
                ));
                writes = true;
            }
        }
        if !writes && want.read_only {
            out.push(format!("{at}&action=read"));
        }
    }
    out
}

/// A granted scope as the token carries it: a permission set, ours or
/// another app's (as `sets` holds it), as the scopes it lists,
/// `authority=self` as the group, a space scope naming no collection as
/// naming every one, and a blob scope as a scope of each type it takes,
/// written as atproto writes one ([`blob_scopes`]).
pub fn concrete(granted: &[String], group: &str, sets: &Sets) -> String {
    let mut out: Vec<String> = Vec::new();
    for s in granted {
        let scopes = match (set_of(s), sets.get(s)) {
            (Some(set), _) => scopes_of(set),
            (None, Some(set)) => included(s, set),
            (None, None) => vec![s.clone()],
        };
        for s in scopes {
            let mut s = s.replace("authority=self", &format!("authority={group}"));
            let every = |w: SpaceScope| w.collections.is_empty() && !w.read_only;
            if SpaceScope::parse(&s).is_some_and(|w| w.manage.is_empty() && every(w)) {
                s.push_str(if s.contains('?') {
                    "&collection=*"
                } else {
                    "?collection=*"
                });
            }
            let each = match Scope::parse(&s) {
                Scope::Blob(b) => blob_scopes(&b),
                _ => vec![s],
            };
            for s in each {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out.join(" ")
}

/// The scopes a permission set lists: a space permission as a space
/// scope, or two where it both writes records and manages the space (the
/// second reading, and managing), and an rpc one as a scope for each of
/// its methods.
pub(crate) fn scopes_of(set: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for p in set["permissions"].as_array().into_iter().flatten() {
        let text = |k: &str| p[k].as_str().unwrap_or_default().to_string();
        if p["resource"] == "rpc" {
            let aud = text("aud");
            out.extend(strings(&p["lxm"]).map(|m| format!("rpc:{m}?aud={aud}")));
            continue;
        }
        let space = |q: &[String]| format!("space:{}?{}", text("spaceType"), q.join("&"));
        let listed = move |key: &'static str| strings(&p[key]).map(move |v| format!("{key}={v}"));
        let at = vec![
            format!("authority={}", text("authority")),
            format!("skey={}", text("skey")),
        ];
        let mut q = at.clone();
        q.extend(listed("collection").chain(listed("action")));
        let manage: Vec<String> = listed("manage").collect();
        if !manage.is_empty() && p["collection"].is_array() {
            out.push(space(&q));
            q = at;
            q.push("action=read".into());
        }
        q.extend(manage);
        out.push(space(&q));
    }
    out
}

fn strings(value: &Value) -> impl Iterator<Item = &str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}
