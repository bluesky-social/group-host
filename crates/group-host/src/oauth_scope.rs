//! What a token signing in as a group may do: the scope an app asks for,
//! narrowed to what the approving member's roles let them do as the
//! group, then made concrete, so the token never depends on how a scope
//! is read later.
//!
//! No lexicon is resolved: the only permission sets a token keeps are
//! this host's own for a group's session (`fyi.opensocial.authManageProfile`
//! within `authModerate` within `authManageGroup`), read from their
//! lexicons here, each an upper bound: the parts of it the roles grant
//! are kept. A space scope naming no collection keeps every one, which
//! narrowing allowed only to roles that may write every one there.

use crate::Host;
use crate::group::{members_space, meta_space};
use crate::lexicons::lexicons;
use crate::rbac::{self, LABEL, NS, actions_for, label_writes, repo_collections_for};
use crate::scopes::{decoded, is_blob, repo_collections};
use crate::xrpc::XrpcError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub use crate::rbac::WRITES;

/// This host's permission sets for a group's session, by name, each
/// within the next.
pub const TIERS: [&str; 3] = ["authManageProfile", "authModerate", "authManageGroup"];

/// The action each of the host's methods a permission set lists asks of
/// the member who signed the app in.
const ASKS: [(&str, &str); 7] = [
    ("listJoinRequests", "admit"),
    ("denyJoinRequest", "admit"),
    ("listInvites", "invite"),
    ("listModSubjects", "mod.read"),
    ("getModSubjectHistory", "mod.read"),
    ("getModSubjectBlob", "mod.read"),
    ("resolveModSubject", "mod.resolve"),
];

/// The permission set of this host's that `scope` includes, from its
/// lexicon, if it is one: `include:fyi.opensocial.authModerate`.
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
    pub fn parse(scope: &str) -> Option<SpaceScope> {
        let scope = decoded(scope);
        let rest = scope.strip_prefix("space:")?;
        let (kind, query) = rest.split_once('?').unwrap_or((rest, ""));
        let mut values: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (k, v) in query.split('&').filter_map(|kv| kv.split_once('=')) {
            values.entry(k).or_default().push(v);
        }
        let all = |k: &str| values.get(k).cloned().unwrap_or_default();
        let one = |k: &str| all(k).first().map_or("*", |v| v).to_string();
        let actions = all("action");
        let named: Vec<String> = WRITES
            .iter()
            .filter(|w| actions.contains(w))
            .map(|w| w.to_string())
            .collect();
        let writes = if named.is_empty() {
            WRITES.map(String::from).to_vec()
        } else {
            named
        };
        let manage: Vec<String> = all("manage").iter().map(|m| m.to_string()).collect();
        let read_only = manage.is_empty()
            && !actions.is_empty()
            && actions.iter().all(|a| *a == "read" || *a == "read_self");
        Some(SpaceScope {
            kind: kind.into(),
            authority: one("authority"),
            skey: one("skey"),
            collections: all("collection").iter().map(|c| c.to_string()).collect(),
            writes,
            read_only,
            manage,
        })
    }

    /// Whether it may be for `group`'s spaces.
    fn for_group(&self, group: &str) -> bool {
        ["*", "self", group].contains(&self.authority.as_str())
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
/// first two segments, as `fyi.opensocial`.
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
                writable.extend(meta_writes(actions));
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

    /// The scopes of `requested` a holder of `roles` in `group` may give
    /// an app acting as the group:
    ///
    /// - `atproto`, always;
    /// - one of this host's permission sets for a group's session
    ///   ([`TIERS`]), as far as the roles grant it ([`parts`]);
    /// - `repo:C`, for roles whose grants hold `repo:C` or `repo:*`;
    /// - `rpc:M`, a method of another service, for roles with `repo:*`,
    ///   never one of this host's own;
    /// - `space:T?…`, where some space of the group's of type `T` lets the
    ///   roles write each collection asked for, as asked, as the group; or
    ///   read it, if reading is all it asks;
    /// - `space:fyi.opensocial.invites?…`, people's inboxes, for roles that
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
            let space = SpaceScope::parse(s).filter(|w| w.for_group(group));
            let keep = if s == "atproto" {
                true
            } else if let Some(set) = tier_of(s) {
                each.push(parts(set, &held, &rooms));
                continue;
            } else if let Some(named) = repo_collections(s) {
                let each = !named.is_empty() && named.iter().all(|c| repo.contains(c));
                everything || each
            } else if let Some(m) = s.strip_prefix("rpc:") {
                let method = m.split('?').next().unwrap_or_default();
                everything && method != "*" && !method.starts_with(&format!("{NS}."))
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
            tier_of(s).is_some()
                || repo_collections(s).is_some()
                || SpaceScope::parse(s).is_some_and(|w| !w.read_only)
        };
        let any_write = each.iter().flatten().any(writes);
        let mut out: Vec<String> = Vec::new();
        for (s, mut gives) in requested.iter().zip(each) {
            if any_write && is_blob(s) {
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

/// What a holder of `actions` may write as the group in its meta space,
/// by collection, beyond what its policy grants: every record of it, if
/// they run the group.
pub fn meta_writes(actions: &BTreeSet<&'static str>) -> BTreeMap<String, BTreeSet<String>> {
    let all: BTreeSet<String> = WRITES.map(String::from).into();
    let records = ["profile", "rule", "labeler"].map(|r| (format!("{NS}.{r}"), all.clone()));
    match actions.contains("group.configure") {
        true => records.into(),
        false => BTreeMap::new(),
    }
}

/// Of `set`, one of [`TIERS`], what holders of `actions`, with `rooms`,
/// grant: each of its methods whose action they hold; of its space
/// permissions, what they may write of each collection it names, and do
/// to the space itself. All they grant of a tier is that tier.
fn parts(set: &Value, actions: &BTreeSet<&'static str>, rooms: &[Room]) -> Vec<String> {
    let mut out = Vec::new();
    for scope in scopes_of(set) {
        if let Some(rpc) = scope.strip_prefix(&format!("rpc:{NS}.")) {
            let method = rpc.split('?').next().unwrap_or_default();
            let asks = ASKS.iter().find(|(m, _)| *m == method);
            if asks.is_some_and(|(_, a)| actions.contains(a)) {
                out.push(scope);
            }
            continue;
        }
        let want = SpaceScope::parse(&scope).expect("a set's permissions are rpc or space");
        let whole = if !want.manage.is_empty() {
            manages(&want, actions, rooms)
        } else if want.kind == format!("{NS}.invites") {
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
    let tier = TIERS.iter().map(|t| format!("include:{NS}.{t}")).find(|t| {
        let set = set_of(t).expect("each tier has its lexicon");
        scopes_of(set) == out
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
    SpaceScope::parse(scope).filter(|w| w.kind == format!("{NS}.invites"))
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

/// A granted scope as the token carries it: a permission set of ours as
/// the scopes its lexicon lists, `authority=self` as the group, and a
/// space scope naming no collection as naming every one.
pub fn concrete(granted: &[String], group: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for s in granted {
        let scopes = match set_of(s) {
            Some(set) => scopes_of(set),
            None => vec![s.clone()],
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
            if !out.contains(&s) {
                out.push(s);
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
