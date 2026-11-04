//! What a token signing in as a group may do: the scope an app asks for,
//! narrowed to what the approving member's roles let them do as the
//! group, then made concrete, so the token never depends on how a scope
//! is read later.
//!
//! As the prototype's `pds/oauth/scope.ts`, except that no lexicon is
//! resolved: the only permission set a token keeps is this host's own
//! (`fyi.opensocial.configure`), read from its lexicon here, and a space
//! scope naming no collection keeps every one, which narrowing allowed
//! only to roles that may write every one there.

use crate::Host;
use crate::group::members_space;
use crate::rbac::{Access, NS, actions_for, grants, listed, repo_collections_for};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The writes a space scope may name.
pub const WRITES: [&str; 3] = ["create", "update", "delete"];

/// The permission set that lets an app configure the group.
pub fn configure() -> String {
    format!("include:{NS}.configure")
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

/// What a holder of `roles` may write as the group in a space, by
/// collection (`*` for every one), by the union of their grants in its
/// access record's `credentialScopes`.
pub fn writes_for(access: &Value, roles: &[String]) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let scopes = access["credentialScopes"].as_array().into_iter().flatten();
    for grant in scopes.filter(|g| roles.iter().any(|r| g["role"] == r.as_str())) {
        for scope in listed(grant, "scopes", "collection") {
            let (collection, query) = scope.split_once('?').unwrap_or((&scope, ""));
            let named: Vec<&str> = query
                .split('&')
                .filter_map(|kv| kv.strip_prefix("action="))
                .filter(|a| WRITES.contains(a))
                .collect();
            let actions = if named.is_empty() {
                WRITES.to_vec()
            } else {
                named
            };
            let has = out.entry(collection.into()).or_default();
            has.extend(actions.iter().map(|a| a.to_string()));
        }
    }
    out
}

/// Whether those writes include `action` on `collection`.
fn may_write(writes: &BTreeMap<String, BTreeSet<String>>, collection: &str, action: &str) -> bool {
    ["*", collection]
        .iter()
        .any(|c| writes.get(*c).is_some_and(|w| w.contains(action)))
}

/// Whether a kind of record and a kind of space are one app's: the same
/// first two segments, as `dev.knotless`.
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
            let rules = Access::of(&access);
            let readable = rules.can_read(roles, member);
            // A space for chosen people is written as the group by those
            // of them who may do anything as it.
            let writable = match rules.people {
                Some(_) if readable && everything => {
                    BTreeMap::from([("*".into(), WRITES.map(String::from).into())])
                }
                Some(_) => BTreeMap::new(),
                None => writes_for(&access, roles),
            };
            rooms.push(Room {
                kind: space.kind,
                skey: space.skey,
                readable,
                writable,
            });
        }
        Ok(rooms)
    }

    /// The scopes of `requested` a holder of `roles` in `group`, `member`,
    /// may give an app acting as the group:
    ///
    /// - `atproto`, always;
    /// - the configure permission set, for roles that configure the group;
    /// - `repo:C`, for roles whose grants hold `repo:C` or `repo:*`;
    /// - `rpc:M`, a method of another service, for roles with `repo:*`,
    ///   never one of this host's own;
    /// - `space:T?…`, where some space of the group's of type `T` lets the
    ///   roles write each collection asked for, as asked, as the group; or
    ///   read it, if reading is all it asks;
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
        let configures = actions_for(&grants, roles).contains("group.configure");
        let rooms = self.rooms(group, roles, member, everything)?;
        let (mut kept, mut made) = (Vec::new(), Vec::new());
        for s in requested {
            let space = SpaceScope::parse(s).filter(|w| w.for_group(group));
            let keep = if s == "atproto" {
                true
            } else if *s == configure() {
                configures
            } else if let Some(c) = s.strip_prefix("repo:") {
                everything || repo.contains(c.split('?').next().unwrap_or_default())
            } else if let Some(m) = s.strip_prefix("rpc:") {
                let method = m.split('?').next().unwrap_or_default();
                everything && method != "*" && !method.starts_with(&format!("{NS}."))
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
            *s == configure()
                || s.starts_with("repo:")
                || SpaceScope::parse(s).is_some_and(|w| !w.read_only)
        };
        let any_write = kept.iter().chain(&made).any(writes);
        let mut out: Vec<String> = requested
            .iter()
            .filter(|s| kept.contains(s) || (any_write && s.starts_with("blob:")))
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

/// A granted scope as the token carries it: the configure set as the
/// space scopes its lexicon lists, `authority=self` as the group, and a
/// space scope naming no collection as naming every one.
pub fn concrete(granted: &[String], group: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for s in granted {
        let scopes = if *s == configure() {
            configure_scopes()
        } else {
            vec![s.clone()]
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

/// The space scopes the configure permission set lists, from its lexicon.
fn configure_scopes() -> Vec<String> {
    let lexicon: Value = serde_json::from_str(include_str!(
        "../../../lexicons/fyi/opensocial/configure.json"
    ))
    .expect("the configure lexicon is JSON");
    let permissions = lexicon["defs"]["main"]["permissions"]
        .as_array()
        .into_iter()
        .flatten();
    permissions
        .map(|p| {
            let text = |k: &str| p[k].as_str().unwrap_or_default().to_string();
            let mut q = vec![
                format!("authority={}", text("authority")),
                format!("skey={}", text("skey")),
            ];
            q.extend(strings(&p["collection"]).map(|c| format!("collection={c}")));
            q.extend(strings(&p["action"]).map(|a| format!("action={a}")));
            format!("space:{}?{}", text("spaceType"), q.join("&"))
        })
        .collect()
}

fn strings(value: &Value) -> impl Iterator<Item = &str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}
