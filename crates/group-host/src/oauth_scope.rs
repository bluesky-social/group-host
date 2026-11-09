//! What a token signing in as a group may do: the scope an app asks for,
//! narrowed to what the approving member's roles let them do as the
//! group, then made concrete, so the token never depends on how a scope
//! is read later.
//!
//! No lexicon is resolved: the only permission sets a token keeps are
//! this host's own for a group's session (`fyi.opensocial.configure`,
//! `manageMembers`, `manageSpaces`, `moderate`), read from their lexicons
//! here, and a space scope naming no collection keeps every one, which
//! narrowing allowed only to roles that may write every one there.

use crate::Host;
use crate::group::members_space;
use crate::lexicons::lexicons;
use crate::rbac::{self, Access, LABEL, NS, actions_for, grants, repo_collections_for};
use crate::scopes::{decoded, is_blob, repo_collections};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub use crate::rbac::WRITES;

/// This host's permission sets for a group's session, each by its name,
/// with the actions a member's roles must hold, every one, to give it.
pub const SETS: [(&str, &[&str]); 4] = [
    ("configure", &["group.configure"]),
    (
        "manageMembers",
        &["admit", "eject", "role.assign", "invite"],
    ),
    ("manageSpaces", &["space.configure"]),
    ("moderate", &["mod.read", "mod.resolve", "label"]),
];

/// The permission set of this host's that `scope` includes, from its
/// lexicon, if it is one: `include:fyi.opensocial.manageMembers`.
pub fn set_of(scope: &str) -> Option<&'static Value> {
    let name = scope.strip_prefix(&format!("include:{NS}."))?;
    let set = lexicons().def(&format!("{NS}.{name}"));
    set.filter(|d| d["type"] == "permission-set")
}

/// The actions a member's roles must hold to give an app `scope`, if it
/// is one of [`SETS`].
fn needs(scope: &str) -> Option<&'static [&'static str]> {
    let name = scope.strip_prefix(&format!("include:{NS}."))?;
    SETS.iter().find(|(n, _)| *n == name).map(|(_, a)| *a)
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
        let read_only = all("manage").is_empty()
            && !actions.is_empty()
            && actions.iter().all(|a| *a == "read" || *a == "read_self");
        Some(SpaceScope {
            kind: kind.into(),
            authority: one("authority"),
            skey: one("skey"),
            collections: all("collection").iter().map(|c| c.to_string()).collect(),
            writes,
            read_only,
        })
    }

    /// Whether it may be for `group`'s spaces.
    fn for_group(&self, group: &str) -> bool {
        ["*", "self", group].contains(&self.authority.as_str())
    }
}

/// What a holder of `roles`, `member`, may write as the group in a space
/// whose access record is `access`, by collection, `everything` being
/// whether they may do anything as the group. A space for chosen people
/// is written as the group by those of them who may do anything as it;
/// any other, as its grants say.
pub fn writes_in(
    access: &Access,
    roles: &[String],
    member: &str,
    everything: bool,
) -> BTreeMap<String, BTreeSet<String>> {
    match access.people {
        Some(_) if everything && access.can_read(roles, member) => {
            BTreeMap::from([("*".into(), WRITES.map(String::from).into())])
        }
        Some(_) => BTreeMap::new(),
        None => access.writes(roles),
    }
}

/// Whether a holder of `roles`, `member`, labelling (`labels`), writes
/// labels as the group in a space of `access`: in any of the group's
/// spaces, but one for chosen people only as one of them.
pub fn labels_in(access: &Access, roles: &[String], member: &str, labels: bool) -> bool {
    labels && (access.people.is_none() || access.can_read(roles, member))
}

/// `writable` with labels, if `labels` and it does not hold every write
/// of them already.
pub fn with_labels(writable: &mut BTreeMap<String, BTreeSet<String>>, labels: bool) {
    if labels && !WRITES.iter().all(|w| may_write(writable, LABEL, w)) {
        writable.insert(LABEL.into(), WRITES.map(String::from).into());
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
}

impl Host {
    /// The group's spaces, each with what a holder of `roles`, `member`,
    /// may read and write there as the group.
    fn rooms(
        &self,
        group: &str,
        roles: &[String],
        member: &str,
        everything: bool,
        labels: bool,
    ) -> Result<Vec<Room>, XrpcError> {
        let entries = self
            .group_records(group, &members_space(group), &format!("{NS}.space"))
            .map_err(XrpcError::internal)?;
        let spaces = entries.iter().filter_map(|(_, entry)| {
            let uri = entry["space"].as_str()?;
            Some((uri, SpaceRef::parse(uri).ok()?))
        });
        let mut rooms = Vec::new();
        for (uri, space) in spaces {
            let access = self
                .group_record(group, uri, &format!("{NS}.access"), "self")
                .map_err(XrpcError::internal)?
                .unwrap_or_default();
            let access = Access::of(&access, uri);
            let mut writable = writes_in(&access, roles, member, everything);
            // The members space's own records, as the group grants them.
            if uri == members_space(group) {
                writable.extend(self.members_writes_of(group, roles)?);
            }
            with_labels(&mut writable, labels_in(&access, roles, member, labels));
            rooms.push(Room {
                kind: space.kind,
                skey: space.skey,
                readable: access.can_read(roles, member),
                writable,
            });
        }
        Ok(rooms)
    }

    /// Whether a holder of `roles` under `grants`, `member`, may write
    /// anything as `group` now: in its repo, its own spaces, or another.
    pub fn writes_anything(
        &self,
        group: &str,
        roles: &[String],
        grants: &[rbac::Grant],
        member: &str,
    ) -> Result<bool, XrpcError> {
        let repo = repo_collections_for(grants, roles);
        if !repo.is_empty() || actions_for(grants, roles).contains("group.configure") {
            return Ok(true);
        }
        let labels = actions_for(grants, roles).contains("label");
        let rooms = self.rooms(group, roles, member, false, labels)?;
        Ok(rooms.iter().any(|r| !r.writable.is_empty()))
    }

    /// The scopes of `requested` a holder of `roles` in `group`, `member`,
    /// may give an app acting as the group:
    ///
    /// - `atproto`, always;
    /// - one of this host's permission sets for a group's session, for
    ///   roles holding every action it needs ([`SETS`]);
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
    /// - `blob:`, beside anything above that writes.
    ///
    /// Everything else is withheld.
    pub fn narrow(
        &self,
        group: &str,
        roles: &[String],
        member: &str,
        requested: &[String],
    ) -> Result<Vec<String>, XrpcError> {
        let permissions = self
            .group_record(
                group,
                &members_space(group),
                &format!("{NS}.permissions"),
                "self",
            )
            .map_err(XrpcError::internal)?
            .unwrap_or_default();
        let grants = grants(&permissions);
        let repo = repo_collections_for(&grants, roles);
        let everything = repo.contains("*");
        let held = actions_for(&grants, roles);
        let rooms = self.rooms(group, roles, member, everything, held.contains("label"))?;
        let invite = format!("{NS}.invite");
        let (mut kept, mut made) = (Vec::new(), Vec::new());
        for s in requested {
            let space = SpaceScope::parse(s).filter(|w| w.for_group(group));
            let keep = if s == "atproto" {
                true
            } else if let Some(needs) = needs(s) {
                needs.iter().all(|a| held.contains(a))
            } else if let Some(named) = repo_collections(s) {
                let each = !named.is_empty() && named.iter().all(|c| repo.contains(c));
                everything || each
            } else if let Some(m) = s.strip_prefix("rpc:") {
                let method = m.split('?').next().unwrap_or_default();
                everything && method != "*" && !method.starts_with(&format!("{NS}."))
            } else if let Some(want) = inbox_scope(s) {
                held.contains("invite") && want.collections.iter().all(|c| *c == invite)
            } else if let Some(want) = space.as_ref().filter(|w| w.kind == "*") {
                made.extend(any_kind(group, want, &rooms));
                false
            } else if let Some(want) = &space {
                rooms.iter().any(|room| allows(want, room))
            } else {
                false
            };
            if keep {
                kept.push(s.clone());
            }
        }
        let writes = |s: &String| {
            needs(s).is_some()
                || repo_collections(s).is_some()
                || SpaceScope::parse(s).is_some_and(|w| !w.read_only)
        };
        let any_write = kept.iter().chain(&made).any(writes);
        let mut out: Vec<String> = requested
            .iter()
            .filter(|s| kept.contains(s) || (any_write && is_blob(s)))
            .cloned()
            .collect();
        for m in made {
            if !out.contains(&m) {
                out.push(m);
            }
        }
        Ok(out)
    }
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
            if SpaceScope::parse(&s).is_some_and(|w| w.collections.is_empty() && !w.read_only) {
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
/// scope, and an rpc one as a scope for each of its methods.
pub(crate) fn scopes_of(set: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for p in set["permissions"].as_array().into_iter().flatten() {
        let text = |k: &str| p[k].as_str().unwrap_or_default().to_string();
        if p["resource"] == "rpc" {
            let aud = text("aud");
            out.extend(strings(&p["lxm"]).map(|m| format!("rpc:{m}?aud={aud}")));
            continue;
        }
        let mut q = vec![
            format!("authority={}", text("authority")),
            format!("skey={}", text("skey")),
        ];
        for key in ["collection", "action", "manage"] {
            q.extend(strings(&p[key]).map(|v| format!("{key}={v}")));
        }
        out.push(format!("space:{}?{}", text("spaceType"), q.join("&")));
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
