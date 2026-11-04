//! What a token's scope lets it do, asked at each use: write a kind of
//! record in the group's repo, upload a blob of a type, or read or write
//! in a space. A token's scope is concrete (`oauth_scope.rs` made it so),
//! and read as atproto's permission spec reads one, as pds.js does: a
//! space scope's authority is `self` (the token's group) unless named,
//! and a scope naming no action grants every one but `read_self`, which
//! `read` holds.

use atproto::space::SpaceRef;

/// The writes a repo scope may name.
const REPO_ACTIONS: [&str; 3] = ["create", "update", "delete"];
/// What a space scope grants when it names no action.
const SPACE_DEFAULT: [&str; 4] = ["read", "create", "update", "delete"];
const SPACE_ACTIONS: [&str; 5] = ["read_self", "read", "create", "update", "delete"];

/// The values of `name` in a scope's query.
fn query<'a>(scope: &'a str, name: &str) -> Vec<&'a str> {
    let q = scope.split_once('?').map_or("", |(_, q)| q);
    q.split('&')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| *k == name)
        .map(|(_, v)| v)
        .collect()
}

/// The resource a scope names after its prefix and colon, if any:
/// `repo:app.bsky.feed.post?…` names `app.bsky.feed.post`.
fn named<'a>(scope: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = scope.strip_prefix(prefix)?;
    let resource = rest.split('?').next().unwrap_or_default();
    match resource.strip_prefix(':') {
        Some(r) => Some(r),
        None => resource.is_empty().then_some(""),
    }
}

/// The actions a scope names, of `known`, or `default` when it names none.
fn actions<'a>(scope: &'a str, known: &[&str], default: &[&'a str]) -> Vec<&'a str> {
    let asked = query(scope, "action");
    if asked.is_empty() {
        return default.to_vec();
    }
    asked.into_iter().filter(|a| known.contains(a)).collect()
}

/// Whether `pattern` (`*/*`, `image/*`, `image/png`) takes `mime`.
fn mime_matches(pattern: &str, mime: &str) -> bool {
    let (p, m) = (pattern.to_lowercase(), mime.to_lowercase());
    if p == "*/*" {
        return true;
    }
    match p.strip_suffix("/*") {
        Some(kind) => m.split('/').next() == Some(kind),
        None => p == m,
    }
}

/// A token's scope, for the group it acts as.
#[derive(Debug, Clone)]
pub struct Permissions {
    scopes: Vec<String>,
    group: String,
}

impl Permissions {
    pub fn new(scope: &str, group: &str) -> Permissions {
        let scopes = scope.split(' ').filter(|s| !s.is_empty()).map(String::from);
        Permissions {
            scopes: scopes.collect(),
            group: group.into(),
        }
    }

    /// May it `action` (`create`, `update`, `delete`) a `collection`
    /// record in the group's repo?
    pub fn repo(&self, collection: &str, action: &str) -> bool {
        self.scopes.iter().any(|s| {
            let Some(resource) = named(s, "repo") else {
                return false;
            };
            let resource = match resource {
                "" => query(s, "collection"),
                r => vec![r],
            };
            let covers = resource.iter().any(|c| *c == "*" || *c == collection);
            covers && actions(s, &REPO_ACTIONS, &REPO_ACTIONS).contains(&action)
        })
    }

    /// May it upload a blob of type `mime`?
    pub fn blob(&self, mime: &str) -> bool {
        self.scopes.iter().any(|s| {
            let Some(resource) = named(s, "blob") else {
                return false;
            };
            let accept = match resource {
                "" => query(s, "accept"),
                r => r.split(',').collect(),
            };
            accept.iter().any(|p| mime_matches(p, mime))
        })
    }

    /// May it `action` (`read`, `read_self`, `create`, `update`,
    /// `delete`) in `space`, on a `collection` record for a write?
    pub fn space(&self, space: &SpaceRef, collection: Option<&str>, action: &str) -> bool {
        self.scopes.iter().any(|s| {
            let Some(kind) = named(s, "space") else {
                return false;
            };
            let kind = if kind.is_empty() {
                query(s, "type").first().copied()
            } else {
                Some(kind)
            };
            let one = |name: &str, default: &'static str| {
                query(s, name).first().copied().unwrap_or(default)
            };
            let authority = match one("authority", "self") {
                "self" => self.group.as_str(),
                a => a,
            };
            let at = kind.is_some_and(|k| k == "*" || k == space.kind)
                && (authority == "*" || authority == space.authority)
                && ["*", space.skey.as_str()].contains(&one("skey", "*"));
            let granted = actions(s, &SPACE_ACTIONS, &SPACE_DEFAULT);
            let may =
                granted.contains(&action) || (action == "read_self" && granted.contains(&"read"));
            if !at || !may {
                return false;
            }
            if action == "read" || action == "read_self" {
                return true;
            }
            let collections = query(s, "collection");
            collection.is_some_and(|c| collections.iter().any(|x| *x == "*" || *x == c))
        })
    }
}
