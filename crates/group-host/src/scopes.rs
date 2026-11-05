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

/// `scope` with its percent-escapes read, as the permission spec reads a
/// scope's values (`authority=did%3Aplc%3Aabc` names `did:plc:abc`), but
/// for an escape of what shapes a scope (`%`, `&`, `=`, `?`), which stays
/// as written so that it cannot split a value.
pub fn decoded(scope: &str) -> String {
    let bytes = scope.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = scope.get(i + 1..i + 3).filter(|_| bytes[i] == b'%');
        let byte = hex.and_then(|h| u8::from_str_radix(h, 16).ok());
        match byte.filter(|b| !b"%&=?".contains(b)) {
            Some(b) => (out.push(b), i += 3),
            None => (out.push(bytes[i]), i += 1),
        };
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The collections a repo scope names, `repo:C` or `repo?collection=C…`,
/// if it is one.
pub fn repo_collections(scope: &str) -> Option<Vec<String>> {
    let scope = decoded(scope);
    let named = match named(&scope, "repo")? {
        "" => query(&scope, "collection"),
        r => vec![r],
    };
    Some(named.into_iter().map(String::from).collect())
}

/// Whether `scope` is a blob scope, `blob:…` or `blob?accept=…`.
pub fn is_blob(scope: &str) -> bool {
    named(scope, "blob").is_some()
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
        let scopes = scope.split(' ').filter(|s| !s.is_empty()).map(decoded);
        Permissions {
            scopes: scopes.collect(),
            group: group.into(),
        }
    }

    /// Whether it holds `transition:generic`, as an app password's session
    /// does: the group's repo and blobs, whole, and never a space.
    fn generic(&self) -> bool {
        self.scopes.iter().any(|s| s == "transition:generic")
    }

    /// May it `action` (`create`, `update`, `delete`) a `collection`
    /// record in the group's repo?
    pub fn repo(&self, collection: &str, action: &str) -> bool {
        self.generic()
            || self.scopes.iter().any(|s| {
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
        self.generic()
            || self.scopes.iter().any(|s| {
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

/// Whether the scopes `held` grant all that `scope` does, for `group`:
/// the same scope, or one as wide or wider. Permission sets are to be
/// made into their scopes first, on both sides. A scope of a kind not
/// read here is held only as written.
pub fn covers(held: &[String], scope: &str, group: &str) -> bool {
    let scope = decoded(scope);
    let held: Vec<String> = held.iter().map(|h| decoded(h)).collect();
    if held.contains(&scope) {
        return true;
    }
    // A scope that manages spaces reads none here.
    let plain: Vec<&str> = held
        .iter()
        .filter(|h| query(h, "manage").is_empty())
        .map(String::as_str)
        .collect();
    let p = Permissions::new(&plain.join(" "), group);
    if let Some(resource) = named(&scope, "repo") {
        let collections = match resource {
            "" => query(&scope, "collection"),
            r => vec![r],
        };
        let writes = actions(&scope, &REPO_ACTIONS, &REPO_ACTIONS);
        let each = |c: &&str| writes.iter().all(|a| p.repo(c, a));
        return !collections.is_empty() && collections.iter().all(each);
    }
    if let Some(kind) = named(&scope, "space") {
        let kind = match kind {
            "" => query(&scope, "type").first().copied().unwrap_or("*"),
            k => k,
        };
        return space_covered(&held, &p, &scope, kind, group);
    }
    if let Some(method) = named(&scope, "rpc") {
        let aud = query(&scope, "aud");
        return held.iter().any(|h| {
            let theirs = named(h, "rpc").is_some_and(|m| m == method || m == "*");
            let to = query(h, "aud");
            theirs && (to.contains(&"*") || to == aud)
        });
    }
    if let Some(accept) = named(&scope, "blob") {
        let accept = match accept {
            "" => query(&scope, "accept"),
            a => a.split(',').collect(),
        };
        return !accept.is_empty() && accept.iter().all(|m| p.blob(m));
    }
    false
}

/// Whether `held`, or `p` (what of it does not manage spaces), grant the
/// space scope `scope`, of spaces of `kind`.
fn space_covered(held: &[String], p: &Permissions, scope: &str, kind: &str, group: &str) -> bool {
    let manage = query(scope, "manage");
    if !manage.is_empty() {
        return manage.iter().all(|m| {
            held.iter().any(|h| {
                let at = named(h, "space").is_some_and(|k| k == "*" || k == kind);
                at && query(h, "manage").contains(m)
            })
        });
    }
    let one = |name: &str, default: &'static str| {
        let v = query(scope, name);
        v.first().copied().unwrap_or(default).to_string()
    };
    let authority = match one("authority", "self").as_str() {
        "self" => group.to_string(),
        a => a.to_string(),
    };
    let at = SpaceRef {
        authority,
        kind: kind.into(),
        skey: one("skey", "*"),
    };
    let collections = query(scope, "collection");
    let collections = if collections.is_empty() {
        vec!["*"]
    } else {
        collections
    };
    let granted = actions(scope, &SPACE_ACTIONS, &SPACE_DEFAULT);
    let each = |a: &&str| collections.iter().all(|c| p.space(&at, Some(c), a));
    granted.iter().all(each)
}
