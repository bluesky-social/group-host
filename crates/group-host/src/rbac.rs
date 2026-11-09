//! What a group's records let each of its people do, as pure functions
//! over those records: a space's access record says who reads it and who
//! writes in it; the permissions record grants each role OAuth scopes, and
//! each action the host's methods ask about is answered by one scope.
//! Roles compose by union: no deny, no precedence, no hierarchy.
//!
//! The records' lists are of objects (`{role}`, `{did}`), but for the
//! scopes and assignable roles of a grant, and a membership's roles,
//! which are strings alone, as the proposal has them.

use crate::scopes::{Permissions, decoded};
use atproto::space::SpaceRef;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const NS: &str = "fyi.opensocial";
/// A label, as a record the group writes beside its subject.
pub const LABEL: &str = "com.atproto.label.label";
/// Anyone signed in, as an audience.
pub const PUBLIC: &str = "public";

/// The actions the host's methods ask about.
pub const ACTIONS: [&str; 11] = [
    "mod.read",
    "mod.resolve",
    "label",
    "invite",
    "admit",
    "eject",
    "role.assign",
    "space.create",
    "space.configure",
    "space.delete",
    "group.configure",
];

/// The values of `field` in each object of the list at `key`, as
/// `[{role: "admin"}]` holds `admin`.
pub fn listed(record: &Value, key: &str, field: &str) -> Vec<String> {
    record[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i[field].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// The roles a membership record gives, each by its id, as the
/// opensocial proposal lists them: `["member", "admin"]`.
pub fn roles_of(membership: &Value) -> Vec<String> {
    let roles = membership["roles"].as_array().into_iter().flatten();
    roles.filter_map(Value::as_str).map(String::from).collect()
}

/// A role's grant in a space's access record: the OAuth scopes it holds
/// there, which count only in that space.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceGrant {
    pub role: String,
    pub scopes: Vec<String>,
}

/// A space's access record, as read, for the space it is in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Access {
    /// The space it is the record of; a grant counts in no other.
    pub space: Option<SpaceRef>,
    /// Whether anyone signed in reads it, member or not.
    pub public: bool,
    pub grants: Vec<SpaceGrant>,
    /// The people it is for, by DID, when it is for chosen people.
    pub people: Option<Vec<String>>,
    /// Who may write in it in their own name, when not every member whose
    /// roles grant `read`.
    pub writable_by: Option<Vec<String>>,
}

impl Access {
    /// The access record `record` of the space at `space`.
    pub fn of(record: &Value, space: &str) -> Access {
        let maybe = |key: &str, field: &str| record.get(key).map(|_| listed(record, key, field));
        let grants = record["grants"].as_array().into_iter().flatten();
        Access {
            space: SpaceRef::parse(space).ok(),
            public: record["public"] == true,
            grants: grants
                .map(|g| SpaceGrant {
                    role: g["role"].as_str().unwrap_or_default().into(),
                    scopes: strings(&g["scopes"]),
                })
                .collect(),
            people: maybe("people", "did"),
            writable_by: maybe("writableBy", "role"),
        }
    }

    /// The scopes a holder of `roles` is granted here, by union.
    pub fn scopes(&self, roles: &[String]) -> Vec<String> {
        let mine = self.grants.iter().filter(|g| roles.contains(&g.role));
        mine.flat_map(|g| g.scopes.clone()).collect()
    }

    /// Whether `scopes` let their holder `action` here, on `collection`
    /// for a write.
    fn lets(&self, scopes: &[String], collection: Option<&str>, action: &str) -> bool {
        let Some(space) = &self.space else {
            return false;
        };
        let held = Permissions::new(&scopes.join(" "), &space.authority);
        held.space(space, collection, action)
    }

    /// Whether `roles` are granted `read` here.
    pub fn reads(&self, roles: &[String]) -> bool {
        self.lets(&self.scopes(roles), None, "read")
    }

    /// What a holder of `roles` may write here as the group, by collection
    /// (`*` for every one): the writes of each collection their scopes
    /// name.
    pub fn writes(&self, roles: &[String]) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for scope in self.scopes(roles) {
            let one = [scope.clone()];
            for collection in query_of(&decoded(&scope), "collection") {
                for action in WRITES {
                    if self.lets(&one, Some(&collection), action) {
                        out.entry(collection.clone())
                            .or_default()
                            .insert(action.into());
                    }
                }
            }
        }
        out
    }

    /// May `did`, holding `roles`, read the space? Anyone signed in, if it
    /// is public; else those whose roles grant `read`. One for chosen
    /// people is for them while they are members, and those whose roles
    /// grant `read`.
    pub fn can_read(&self, roles: &[String], did: &str) -> bool {
        if let Some(people) = &self.people {
            let theirs = !roles.is_empty() && people.iter().any(|p| p == did);
            return theirs || self.reads(roles);
        }
        self.public || self.reads(roles)
    }

    /// May `did`, holding `roles`, write in the space in their own name?
    /// A member whose roles grant `read`, unless the record names who.
    pub fn can_write_own(&self, roles: &[String], did: &str) -> bool {
        if let Some(people) = &self.people {
            return !roles.is_empty() && people.iter().any(|p| p == did);
        }
        match &self.writable_by {
            None => !roles.is_empty() && self.reads(roles),
            Some(by) => by.iter().any(|r| r == PUBLIC || roles.contains(r)),
        }
    }
}

/// The writes a space scope may name.
pub const WRITES: [&str; 3] = ["create", "update", "delete"];

/// The strings in a list of them.
fn strings(value: &Value) -> Vec<String> {
    let items = value.as_array().into_iter().flatten();
    items.filter_map(|s| s.as_str().map(String::from)).collect()
}

/// The values of `name` in a scope's query, owned.
fn query_of(scope: &str, name: &str) -> Vec<String> {
    query(scope, name).into_iter().map(String::from).collect()
}

/// The actions a host method's `rpc:` scope grants, by method.
const BY_METHOD: [(&str, &str); 2] = [
    ("listSubjects", "mod.read"),
    ("resolveSubject", "mod.resolve"),
];

/// The scope that grants each action.
pub fn scope_of(action: &str) -> Option<String> {
    let rpc = |method: &str| format!("rpc:{NS}.{method}?aud=*");
    let membership = |write: &str| {
        format!("space:{NS}.members?skey=self&collection={NS}.membership&action={write}")
    };
    Some(match action {
        "group.configure" => format!("include:{NS}.configure"),
        "space.create" => "space:*?manage=create".into(),
        "space.delete" => "space:*?manage=delete".into(),
        "space.configure" => format!("space:*?collection={NS}.access"),
        "admit" => membership("create"),
        "role.assign" => membership("update"),
        "eject" => membership("delete"),
        "invite" => format!("space:{NS}.invites?authority=*&collection={NS}.invite"),
        "mod.read" => rpc("listSubjects"),
        "mod.resolve" => rpc("resolveSubject"),
        "label" => format!("space:*?collection={LABEL}"),
        _ => return None,
    })
}

/// The values of `name` in a scope's query.
fn query<'a>(scope: &'a str, name: &str) -> Vec<&'a str> {
    let q = scope.split_once('?').map_or("", |(_, q)| q);
    q.split('&')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| *k == name)
        .map(|(_, v)| v)
        .collect()
}

/// What a space scope lets its holder do of writing memberships: `admit`
/// for making one, `role.assign` for changing one, `eject` for deleting
/// one, in the members space or in every space.
fn membership_actions(scope: &str, out: &mut BTreeSet<&'static str>) {
    let Some(rest) = scope.strip_prefix("space:") else {
        return;
    };
    let kind = rest.split('?').next().unwrap_or_default();
    let members = kind == "*" || kind == format!("{NS}.members");
    if !members || !query(scope, "manage").is_empty() {
        return;
    }
    let collections = query(scope, "collection");
    let membership = format!("{NS}.membership");
    let any = collections.is_empty() || collections.contains(&"*");
    if !any && !collections.contains(&membership.as_str()) {
        return;
    }
    let named = query(scope, "action");
    let writes = [
        ("create", "admit"),
        ("update", "role.assign"),
        ("delete", "eject"),
    ];
    for (write, action) in writes {
        if named.is_empty() || named.contains(&write) {
            out.insert(action);
        }
    }
}

/// What these scopes let their holder do. A scope that is none of the
/// host's grants nothing.
pub fn actions_of(scopes: &[String]) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for scope in scopes {
        membership_actions(scope, &mut out);
        if *scope == format!("include:{NS}.configure") {
            out.insert("group.configure");
        } else if *scope == format!("include:{NS}.manageMembers") {
            out.extend(["admit", "eject", "role.assign", "invite"]);
        } else if let Some(rpc) = scope.strip_prefix("rpc:") {
            let method = rpc.split('?').next().unwrap_or_default();
            let action = BY_METHOD
                .iter()
                .find(|(m, _)| method == format!("{NS}.{m}"));
            if let Some((_, action)) = action {
                out.insert(*action);
            }
        } else if scope.starts_with(&format!("space:{NS}.invites?")) {
            // Invites written in people's inboxes.
            if query(scope, "collection").contains(&format!("{NS}.invite").as_str()) {
                out.insert("invite");
            }
        } else if scope.starts_with("space:*") {
            let manage = query(scope, "manage");
            if manage.contains(&"create") {
                out.insert("space.create");
            }
            if manage.contains(&"delete") {
                out.insert("space.delete");
            }
            if !manage.is_empty() {
                continue;
            }
            let collections = query(scope, "collection");
            let every = collections.is_empty() || collections.contains(&"*");
            if every || collections.contains(&format!("{NS}.access").as_str()) {
                out.insert("space.configure");
            }
            if every || collections.contains(&LABEL) {
                out.insert("label");
            }
        }
    }
    out
}

/// A role's grant in the permissions record: its scopes and the roles its
/// holders may assign.
#[derive(Debug, Clone, PartialEq)]
pub struct Grant {
    pub role: String,
    pub scopes: Vec<String>,
    pub assignable: Vec<String>,
}

impl Grant {
    /// A grant of `actions` and the public-repo `collections`.
    pub fn with(role: &str, actions: &[&str], assignable: &[&str], collections: &[&str]) -> Grant {
        let mut scopes: Vec<String> = actions.iter().filter_map(|a| scope_of(a)).collect();
        scopes.extend(collections.iter().map(|c| format!("repo:{c}")));
        Grant {
            role: role.into(),
            scopes,
            assignable: assignable.iter().map(|r| r.to_string()).collect(),
        }
    }

    /// As the permissions record keeps it (`defs#groupGrant`).
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "role": self.role,
            "scopes": self.scopes,
            "assignable": self.assignable,
        })
    }
}

/// The grants a permissions record holds.
pub fn grants(permissions: &Value) -> Vec<Grant> {
    permissions["grants"]
        .as_array()
        .map(|grants| {
            grants
                .iter()
                .map(|g| Grant {
                    role: g["role"].as_str().unwrap_or_default().into(),
                    scopes: strings(&g["scopes"]),
                    assignable: strings(&g["assignable"]),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every action a holder of `roles` has, by union.
pub fn actions_for(grants: &[Grant], roles: &[String]) -> BTreeSet<&'static str> {
    let scopes: Vec<String> = grants
        .iter()
        .filter(|g| roles.contains(&g.role))
        .flat_map(|g| g.scopes.clone())
        .collect();
    actions_of(&scopes)
}

/// The public-repo collections a holder of `roles` may write as the group.
pub fn repo_collections_for(grants: &[Grant], roles: &[String]) -> BTreeSet<String> {
    grants
        .iter()
        .filter(|g| roles.contains(&g.role))
        .flat_map(|g| g.scopes.iter())
        .filter_map(|s| s.strip_prefix("repo:"))
        .map(|c| c.split('?').next().unwrap_or_default().to_string())
        .collect()
}
