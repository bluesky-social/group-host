//! What a group's records let each of its people do, as pure functions
//! over those records: a space's access record says who reads it and who
//! writes in it; the permissions record grants each role OAuth scopes, and
//! each action the host's methods ask about is answered by one scope.
//! Roles compose by union: no deny, no precedence, no hierarchy.
//!
//! As the prototype's `access.ts` and `roles.ts` have them, over this
//! host's records, whose lists are of objects (`{role}`, `{scope}`).

use serde_json::Value;
use std::collections::BTreeSet;

pub const NS: &str = "fyi.opensocial";
/// Anyone signed in, as an audience.
pub const PUBLIC: &str = "public";

/// The actions the host's methods ask about.
pub const ACTIONS: [&str; 12] = [
    "mod.read",
    "mod.resolve",
    "label",
    "takedown",
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

/// A space's access record, as read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Access {
    pub readable_by: Vec<String>,
    /// The people it is for, by DID, when it is for chosen people.
    pub people: Option<Vec<String>>,
    /// The roles that may read a space for chosen people, to decide on it.
    pub reviewers: Vec<String>,
    /// Who may write in it in their own name; absent is any member.
    pub writable_by: Option<Vec<String>>,
}

impl Access {
    pub fn of(record: &Value) -> Access {
        let maybe = |key: &str, field: &str| record.get(key).map(|_| listed(record, key, field));
        Access {
            readable_by: listed(record, "readableBy", "role"),
            people: maybe("people", "did"),
            reviewers: listed(record, "reviewers", "role"),
            writable_by: maybe("writableBy", "role"),
        }
    }

    /// May `did`, holding `roles`, read the space? One for chosen people is
    /// for them while they are members, and its reviewers.
    pub fn can_read(&self, roles: &[String], did: &str) -> bool {
        if let Some(people) = &self.people {
            let theirs = !roles.is_empty() && people.iter().any(|p| p == did);
            return theirs || roles.iter().any(|r| self.reviewers.contains(r));
        }
        self.readable_by
            .iter()
            .any(|r| r == PUBLIC || roles.contains(r))
    }

    /// May `did`, holding `roles`, write in the space in their own name?
    /// Any member, unless the record names who.
    pub fn can_write_own(&self, roles: &[String], did: &str) -> bool {
        if let Some(people) = &self.people {
            return !roles.is_empty() && people.iter().any(|p| p == did);
        }
        match &self.writable_by {
            None => !roles.is_empty(),
            Some(by) => by.iter().any(|r| r == PUBLIC || roles.contains(r)),
        }
    }
}

/// The actions a host method's `rpc:` scope grants, by method.
const BY_METHOD: [(&str, &str); 7] = [
    ("assignRoles", "role.assign"),
    ("ejectMember", "eject"),
    ("createInvite", "invite"),
    ("admitMember", "admit"),
    ("listSubjects", "mod.read"),
    ("resolveSubject", "mod.resolve"),
    ("applyLabel", "label"),
];

/// The scope that grants each action.
pub fn scope_of(action: &str) -> Option<String> {
    let rpc = |method: &str| format!("rpc:{NS}.{method}?aud=*");
    Some(match action {
        "group.configure" => format!("include:{NS}.configure"),
        "space.create" => "space:*?manage=create".into(),
        "space.delete" => "space:*?manage=delete".into(),
        "space.configure" => format!("space:*?collection={NS}.access"),
        "role.assign" => rpc("assignRoles"),
        "eject" => rpc("ejectMember"),
        "invite" => rpc("createInvite"),
        "admit" => rpc("admitMember"),
        "mod.read" => rpc("listSubjects"),
        "mod.resolve" => rpc("resolveSubject"),
        "label" => rpc("applyLabel"),
        "takedown" => format!("space:*?collection={NS}.label"),
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

/// What these scopes let their holder do. A scope that is none of the
/// host's grants nothing.
pub fn actions_of(scopes: &[String]) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for scope in scopes {
        if *scope == format!("include:{NS}.configure") {
            out.insert("group.configure");
        } else if let Some(rpc) = scope.strip_prefix("rpc:") {
            let method = rpc.split('?').next().unwrap_or_default();
            let action = BY_METHOD
                .iter()
                .find(|(m, _)| method == format!("{NS}.{m}"));
            if let Some((_, action)) = action {
                out.insert(*action);
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
            if every || collections.contains(&format!("{NS}.label").as_str()) {
                out.insert("label");
                out.insert("takedown");
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

    /// As the permissions record keeps it.
    pub fn to_json(&self) -> Value {
        let objects = |field: &str, items: &[String]| -> Value {
            items
                .iter()
                .map(|i| serde_json::json!({ field: i }))
                .collect()
        };
        serde_json::json!({
            "role": self.role,
            "scopes": objects("scope", &self.scopes),
            "assignable": objects("role", &self.assignable),
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
                    scopes: listed(g, "scopes", "scope"),
                    assignable: listed(g, "assignable", "role"),
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
