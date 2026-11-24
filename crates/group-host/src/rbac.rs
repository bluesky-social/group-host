//! What a group's records let each of its people do, as pure functions
//! over those records: a space's policy says who reads it, who writes in
//! it in their own name, and what each role may do there as the group;
//! each role record grants its role OAuth scopes across the group, and
//! each action the host's methods ask about is answered by one scope.
//! Roles compose by union: no deny, no precedence, no hierarchy.
//!
//! The records' lists are of objects (`{role}`, `{did}`), but for a
//! role's scopes and assignable roles, and a membership's roles, which
//! are strings alone, as the proposal has them.

use crate::oauth_scope::concrete;
use crate::scopes::{Permissions, decoded};
use atproto::space::SpaceRef;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const NS: &str = "fyi.opensocial";
/// A label, as a record the group writes beside its subject.
pub const LABEL: &str = "com.atproto.label.record";
/// The space policy group spaces are kept under, as a policy's `$type`.
pub const POLICY: &str = "fyi.opensocial.policy";

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

/// A role's grant in a space's policy: whether its holders read the space
/// and write in it in their own name, and the OAuth scopes they hold there
/// acting as the group, which count only in that space.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PolicyGrant {
    pub role: String,
    pub read: bool,
    pub write: bool,
    pub scopes: Vec<String>,
}

/// A space's policy (`fyi.opensocial.policy`), as read, for the space it
/// is the policy of.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    /// The space it is the policy of; a grant counts in no other.
    pub space: SpaceRef,
    /// Whether anyone reads it, member or not, signed in or not.
    pub public_read: bool,
    /// Whether it is left out of the group's index.
    pub unlisted: bool,
    pub grants: Vec<PolicyGrant>,
}

impl Policy {
    /// The policy `policy` of `space`.
    pub fn of(policy: &Value, space: &SpaceRef) -> Policy {
        let grants = policy["grants"].as_array().into_iter().flatten();
        Policy {
            space: space.clone(),
            public_read: policy["publicRead"] == true,
            unlisted: policy["unlisted"] == true,
            grants: grants
                .map(|g| PolicyGrant {
                    role: g["role"].as_str().unwrap_or_default().into(),
                    read: g["read"] == true,
                    write: g["write"] == true,
                    scopes: strings(&g["scopes"]),
                })
                .collect(),
        }
    }

    /// The grant of `role`, if it has one.
    pub fn grant(&self, role: &str) -> Option<&PolicyGrant> {
        self.grants.iter().find(|g| g.role == role)
    }

    fn theirs<'a>(&'a self, roles: &'a [String]) -> impl Iterator<Item = &'a PolicyGrant> {
        self.grants.iter().filter(|g| roles.contains(&g.role))
    }

    /// The scopes a holder of `roles` is granted here, by union.
    pub fn scopes(&self, roles: &[String]) -> Vec<String> {
        self.theirs(roles).flat_map(|g| g.scopes.clone()).collect()
    }

    /// Whether `roles` are granted `read` here.
    pub fn reads(&self, roles: &[String]) -> bool {
        self.theirs(roles).any(|g| g.read)
    }

    /// Whether `roles` are granted `write` here: their own records in the
    /// space are in its writer set.
    pub fn writes_own(&self, roles: &[String]) -> bool {
        self.theirs(roles).any(|g| g.write)
    }

    /// May a holder of `roles` read the space? Anyone, if it is public;
    /// else those whose roles grant `read`.
    pub fn can_read(&self, roles: &[String]) -> bool {
        self.public_read || self.reads(roles)
    }

    /// `scopes`, as a token's would be read in this space.
    fn held(&self, scopes: &[String]) -> Permissions {
        Permissions::new(&scopes.join(" "), &self.space.authority)
    }

    /// Whether a holder of `roles` may `op` (`update`, `delete`) the space
    /// itself, by a scope its grant here holds.
    pub fn manages(&self, roles: &[String], op: &str) -> bool {
        self.held(&self.scopes(roles)).manages(&self.space, op)
    }

    /// What a holder of `roles` may write here as the group, by collection
    /// (`*` for every one): the writes of each collection their scopes
    /// name.
    pub fn writes(&self, roles: &[String]) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for scope in self.scopes(roles) {
            let held = self.held(std::slice::from_ref(&scope));
            for collection in query_of(&decoded(&scope), "collection") {
                for action in WRITES {
                    if held.space(&self.space, Some(&collection), action) {
                        out.entry(collection.clone())
                            .or_default()
                            .insert(action.into());
                    }
                }
            }
        }
        out
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
    ("listModSubjects", "mod.read"),
    ("resolveModSubject", "mod.resolve"),
];

/// The actions holding `fyi.opensocial.authModerate` gives, as its
/// permissions do: memberships, invites, join requests and the
/// moderation methods. Its labels are the meta and members spaces' alone
/// ([`label_writes`]), so it gives no `label`, which is labels in every
/// space. `authManageGroup` gives them, and
/// `group.configure`; `authManageProfile` none, as who writes the
/// profile and rules is the meta space's policy's to say.
pub const MODERATES: [&str; 6] = [
    "mod.read",
    "mod.resolve",
    "invite",
    "admit",
    "eject",
    "role.assign",
];

/// The scope that grants each action.
pub fn scope_of(action: &str) -> Option<String> {
    let rpc = |method: &str| format!("rpc:{NS}.{method}?aud=*");
    let membership = |write: &str| {
        format!("space:{NS}.members?skey=self&collection={NS}.membership&action={write}")
    };
    Some(match action {
        "group.configure" => format!("include:{NS}.authManageGroup"),
        "space.create" => "space:*?manage=create".into(),
        "space.delete" => "space:*?manage=delete".into(),
        "space.configure" => "space:*?manage=update".into(),
        "admit" => membership("create"),
        "role.assign" => membership("update"),
        "eject" => membership("delete"),
        "invite" => format!(
            "space:{NS}.invites?authority=*&skey=self&collection={NS}.invite&action=read_self&action=create&action=update&action=delete"
        ),
        "mod.read" => rpc("listModSubjects"),
        "mod.resolve" => rpc("resolveModSubject"),
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
        if *scope == format!("include:{NS}.authManageGroup") {
            out.extend(MODERATES);
            out.insert("group.configure");
        } else if *scope == format!("include:{NS}.authModerate") {
            out.extend(MODERATES);
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
            if manage.contains(&"update") {
                out.insert("space.configure");
            }
            if manage.contains(&"delete") {
                out.insert("space.delete");
            }
            if !manage.is_empty() {
                continue;
            }
            let collections = query(scope, "collection");
            let every = collections.is_empty() || collections.contains(&"*");
            if every || collections.contains(&LABEL) {
                out.insert("label");
            }
        }
    }
    out
}

/// Every role, as a role's `assignable` names them: those made later too.
pub const EVERY_ROLE: &str = "*";

/// A role's group-wide grant, as its record has it: its scopes and the
/// roles its holders may assign.
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

    /// The grant of the role `role` keys, by its `record`.
    pub fn of(role: &str, record: &Value) -> Grant {
        Grant {
            role: role.into(),
            scopes: strings(&record["scopes"]),
            assignable: strings(&record["assignable"]),
        }
    }
}

/// Whether `assignable`, a union of roles' lists, covers `role`: by
/// naming it, or every role.
pub fn assigns(assignable: &BTreeSet<String>, role: &str) -> bool {
    assignable.contains(EVERY_ROLE) || assignable.contains(role)
}

/// The grants of a group's role records, each with its key.
pub fn grants(roles: &[(String, Value)]) -> Vec<Grant> {
    roles.iter().map(|(key, r)| Grant::of(key, r)).collect()
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

/// The writes of labels a holder of `roles` may make as `group` in
/// `space` by their roles' scopes across the group: those of the
/// moderation sets in its meta and members spaces, as the sets'
/// permissions name them; in any of its spaces, by a space scope naming
/// labels or every collection. A space's policy may grant more there.
pub fn label_writes(
    grants: &[Grant],
    roles: &[String],
    group: &str,
    space: &SpaceRef,
) -> BTreeSet<String> {
    let scopes: Vec<String> = grants
        .iter()
        .filter(|g| roles.contains(&g.role))
        .flat_map(|g| g.scopes.clone())
        .collect();
    let held = Permissions::new(&concrete(&scopes, group), group);
    let writes = WRITES
        .into_iter()
        .filter(|w| held.space(space, Some(LABEL), w));
    writes.map(String::from).collect()
}

/// The label writes of [`label_writes`], and those `policy`, the
/// space's, grants `roles` there.
pub fn label_writes_in(grants: &[Grant], roles: &[String], policy: &Policy) -> BTreeSet<String> {
    let space = &policy.space;
    let mut out = label_writes(grants, roles, &space.authority, space);
    let granted = policy.writes(roles);
    let any = ["*", LABEL].iter().filter_map(|c| granted.get(*c));
    out.extend(any.flatten().cloned());
    out
}
