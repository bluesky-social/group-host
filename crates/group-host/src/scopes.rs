//! What a token's scope lets it do, asked at each use: write a kind of
//! record in the group's repo, upload a blob of a type, call a method, or
//! read or write in a space. A token's scope is concrete (`oauth_scope.rs`
//! made it so), and read as atproto's permission spec and its reference
//! read one (`atproto::scope`): a space scope's authority is `self` (the
//! token's group) unless named, a scope naming no action grants every one
//! (but `read_self`, which `read` holds), and a scope its resource's
//! grammar does not take grants nothing.

use crate::rbac::NS;
use atproto::scope::{Blob, Scope, Scopes, Space, SpaceAction, Transition, Write};
use atproto::space::SpaceRef;

/// A token's scope, for the group it acts as.
#[derive(Debug, Clone)]
pub struct Permissions(Scopes);

impl Permissions {
    pub fn new(scope: &str, group: &str) -> Permissions {
        Permissions(Scopes::parse(scope).resolve_self(group))
    }

    /// May it `action` (`create`, `update`, `delete`) a `collection`
    /// record in the group's repo? `transition:generic` may, as an app
    /// password's session does.
    pub fn repo(&self, collection: &str, action: &str) -> bool {
        Write::parse(action).is_some_and(|a| self.0.allows_repo(collection, a))
    }

    /// May it upload a blob of type `mime`?
    pub fn blob(&self, mime: &str) -> bool {
        self.0.allows_blob(mime)
    }

    /// May it `action` (`read`, `read_self`, `create`, `update`,
    /// `delete`) in `space`, on a `collection` record for a write?
    pub fn space(&self, space: &SpaceRef, collection: Option<&str>, action: &str) -> bool {
        SpaceAction::parse(action).is_some_and(|a| self.0.allows_space(space, a, collection))
    }

    /// May it `op` (`create`, `update`, `delete`) `space` itself?
    pub fn manages(&self, space: &SpaceRef, op: &str) -> bool {
        Write::parse(op).is_some_and(|op| self.0.manages_space(space, op))
    }

    /// May it call `method`, one of this host's own, here? An `rpc:`
    /// scope naming it, for any audience or one `ours` says is this host.
    /// A token carries a permission set of ours as the scopes it lists
    /// (`oauth_scope::concrete`), so those it lists count too.
    pub fn calls(&self, method: &str, ours: impl Fn(&str) -> bool) -> bool {
        let names = |l: &Vec<String>| l.iter().any(|l| l == method);
        let calls = |s: &Scope| matches!(s, Scope::Rpc(r) if names(&r.lxms) && (r.aud == "*" || ours(&r.aud)));
        self.0.0.iter().any(calls)
    }

    /// May it call `lxm` of the service `aud` (`did#service`), through
    /// this host? An `rpc:` scope naming the method, or `*`, and the
    /// audience, or `*` (but never both `*`); `transition:generic` any
    /// method but chat's (`chat.bsky.*`), and `transition:chat.bsky`
    /// chat's.
    pub fn rpc(&self, lxm: &str, aud: &str) -> bool {
        self.0.allows_rpc(aud, lxm)
    }
}

/// `scope` read for `group`: a space scope's authority `self` as the
/// group.
fn resolved(scope: &str, group: &str) -> Scope {
    match Scope::parse(scope) {
        Scope::Space(s) => Scope::Space(s.resolve_self(group)),
        s => s,
    }
}

/// Whether the scopes `held` grant all that `scope` does, for `group`:
/// the same scope, or one as wide or wider. Permission sets are to be
/// made into their scopes first, on both sides. A scope of a kind not
/// read here is held only as written.
pub fn covers(held: &[String], scope: &str, group: &str) -> bool {
    let scope = resolved(scope, group);
    let held: Vec<Scope> = held.iter().map(|h| resolved(h, group)).collect();
    if held.contains(&scope) {
        return true;
    }
    // A scope that manages spaces reads none here.
    let manages = |h: &&Scope| matches!(h, Scope::Space(s) if s.manage.is_some());
    let p = Scopes(held.iter().filter(|h| !manages(h)).cloned().collect());
    match &scope {
        Scope::Repo(r) => {
            let each = |c: &String| r.actions.iter().all(|a| p.allows_repo(c, *a));
            r.collections.iter().all(each)
        }
        Scope::Space(want) => space_covered(&held, &p, want),
        Scope::Rpc(want) => want.lxms.iter().all(|lxm| {
            held.iter().any(|h| {
                let Scope::Rpc(h) = h else { return false };
                let theirs = h.lxms.iter().any(|m| m == lxm || m == "*");
                theirs && (h.aud == "*" || h.aud == want.aud)
            })
        }),
        Scope::Blob(want) => want.accept.iter().all(|a| takes(&p, a)),
        _ => false,
    }
}

/// Whether `p` takes uploads of all `accept` names, a type or a pattern
/// of them (`image/*`): `transition:generic`, a blob scope naming it, or
/// a pattern holding it.
fn takes(p: &Scopes, accept: &str) -> bool {
    let holds = |pattern: &String| {
        let kind = pattern.strip_suffix('*').filter(|k| k.ends_with('/'));
        pattern == "*/*" || pattern == accept || kind.is_some_and(|k| accept.starts_with(k))
    };
    let blob = |s: &Scope| matches!(s, Scope::Blob(b) if b.accept.iter().any(holds));
    p.0.contains(&Scope::Transition(Transition::Generic)) || p.0.iter().any(blob)
}

/// Whether `held`, or `p` (what of it does not manage spaces), grant the
/// space scope `want`.
fn space_covered(held: &[Scope], p: &Scopes, want: &Space) -> bool {
    match &want.manage {
        Some(manage) => manage.iter().all(|op| {
            held.iter().any(|h| {
                let Scope::Space(h) = h else { return false };
                let kind = h.space_type == "*" || h.space_type == want.space_type;
                kind && h.manage.iter().flatten().any(|m| m == op)
            })
        }),
        None => {
            let at = SpaceRef {
                authority: want.authority.clone(),
                kind: want.space_type.clone(),
                skey: want.skey.clone(),
            };
            let every = ["*".to_string()];
            let collections = want.collections.as_deref().unwrap_or(&every);
            let each =
                |a: &SpaceAction| collections.iter().all(|c| p.allows_space(&at, *a, Some(c)));
            want.actions.iter().all(each)
        }
    }
}

/// The scopes a token keeps for a blob scope: one for each type it
/// takes, as atproto writes it (lowercase), so that each reads back as
/// it was meant, where a list of them written as one may not.
pub fn blob_scopes(blob: &Blob) -> Vec<String> {
    let one = |a: &String| Blob {
        accept: vec![a.clone()],
    };
    let each = blob.accept.iter().map(|a| Scope::Blob(one(a)).to_string());
    each.collect()
}

/// A token's scope as this host issued it before it read scopes as
/// atproto does, written so that it grants what it did then: a blob
/// scope as [`blob_scopes`] writes it, and each scope atproto's grammar
/// does not take as [`legacy`] reads it.
pub fn upgraded(scope: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for s in scope.split(' ').filter(|s| !s.is_empty()) {
        let each = match Scope::parse(s) {
            Scope::Blob(b) => blob_scopes(&b),
            Scope::Other(_) => legacy(s),
            _ => vec![s.to_string()],
        };
        for s in each {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out.join(" ")
}

/// `s`, a scope atproto's grammar does not take, as this host read it
/// before: `blob:a,b` as a scope of each type, compared ignoring case;
/// of a repo, rpc or space scope, the parameters it does not take, the
/// values of no action and, of a value it takes once, all but the first,
/// left out; and the audience of one of the host's own methods, its DID
/// as it took one bare, as the service it is (`#atgrp`). What still
/// reads as no scope is kept as written.
fn legacy(s: &str) -> Vec<String> {
    let (head, query) = s.split_once('?').unwrap_or((s, ""));
    let (resource, positional) = match head.split_once(':') {
        Some((r, p)) => (r, Some(p)),
        None => (head, None),
    };
    let pairs = query.split('&').filter_map(|kv| kv.split_once('='));
    if resource == "blob" {
        let accept: Vec<String> = match positional {
            Some(p) => p.split(',').map(String::from).collect(),
            None => pairs
                .filter(|(k, _)| *k == "accept")
                .map(|(_, v)| v.into())
                .collect(),
        };
        let blob = Blob { accept };
        let each = blob_scopes(&blob).into_iter();
        return each
            .filter(|b| matches!(Scope::parse(b), Scope::Blob(_)))
            .collect();
    }
    let (keys, named, once): (&[&str], &str, &[&str]) = match resource {
        "repo" => (&["collection", "action"], "collection", &[]),
        "rpc" => (&["lxm", "aud"], "lxm", &["aud"]),
        "space" => (
            &[
                "spaceType",
                "authority",
                "skey",
                "collection",
                "action",
                "manage",
            ],
            "spaceType",
            &["spaceType", "authority", "skey"],
        ),
        _ => return vec![s.into()],
    };
    let word = |k: &str, v: &str| match k {
        "action" if resource == "repo" => Write::parse(v).is_some(),
        "action" => SpaceAction::parse(v).is_some(),
        "manage" => Write::parse(v).is_some(),
        _ => true,
    };
    let mut kept: Vec<(&str, &str)> = Vec::new();
    let mut acts = false;
    for (k, v) in pairs {
        acts |= k == "action";
        let again = once.contains(&k) && kept.iter().any(|(x, _)| *x == k);
        let doubled = positional.is_some() && k == named;
        if keys.contains(&k) && !doubled && !again && word(k, v) {
            kept.push((k, v));
        }
    }
    // Naming only actions there are none of, it granted nothing.
    if acts && !kept.iter().any(|(k, _)| *k == "action") {
        return vec![s.into()];
    }
    let own = resource == "rpc" && positional.is_some_and(|m| m.starts_with(&format!("{NS}.")));
    let bare = |v: &str| v != "*" && !v.contains('#') && !v.contains("%23");
    let value = |k: &str, v: &str| match own && k == "aud" && bare(v) {
        true => format!("{v}%23atgrp"),
        false => v.to_string(),
    };
    let query: Vec<String> = kept
        .iter()
        .map(|(k, v)| format!("{k}={}", value(k, v)))
        .collect();
    let written = match query.is_empty() {
        true => head.to_string(),
        false => format!("{head}?{}", query.join("&")),
    };
    match Scope::parse(&written) {
        Scope::Other(_) => vec![s.into()],
        _ => vec![written],
    }
}

#[cfg(test)]
mod tests {
    use super::upgraded;
    use crate::group_invites::inviting_scope;
    use crate::group_tokens::scopes_for;
    use crate::member_writes::own_scopes;
    use crate::oauth_scope::{TIERS, concrete, scopes_of, set_of};
    use crate::permission_sets::included;
    use crate::rbac::{ACTIONS, NS, scope_of};
    use atproto::scope::Scope;
    use atproto::space::SpaceRef;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};

    /// Every form of scope this host writes, into a token or a role's
    /// record, is a scope of its kind as atproto reads one, and the
    /// migration of tokens' scopes leaves it as it is.
    #[test]
    fn reads_every_scope_the_host_writes() {
        let group = "did:plc:riders";
        let space = SpaceRef::parse("at:space/did:plc:riders/com.example.calendar/rides").unwrap();
        let writes = |a: &[&str]| a.iter().map(|a| a.to_string()).collect::<BTreeSet<_>>();
        let writes = BTreeMap::from([
            (
                "com.example.event".to_string(),
                writes(&["create", "delete"]),
            ),
            ("*".to_string(), writes(&["update"])),
        ]);
        let words = |s: String| s.split(' ').map(String::from).collect::<Vec<_>>();
        let mut forms: Vec<String> = ACTIONS.iter().filter_map(|a| scope_of(a)).collect();
        for set in TIERS.iter().chain(&["authParticipate"]) {
            let set = format!("include:{NS}.{set}");
            forms.extend(scopes_of(set_of(&set).unwrap()));
            forms.extend(words(concrete(&[set], group, &Default::default())));
        }
        forms.extend(scopes_for(&space, &writes));
        forms.extend(own_scopes(&space, &writes));
        forms.extend(words(inviting_scope(&space)));
        let set = json!({"permissions": [
            {"resource": "repo", "collection": ["social.grain.photo"], "action": ["create"]},
            {"resource": "repo", "collection": ["social.grain.like"]},
            {"resource": "rpc", "lxm": ["social.grain.getThing"], "inheritAud": true},
            {"resource": "rpc", "lxm": ["social.grain.getOther"], "aud": "*"},
        ]});
        let include = "include:social.grain.authFull?aud=did:web:grain.social%23grain_appview";
        forms.extend(included(include, &set));
        // An app password's, and what narrowing writes of a set's space
        // permission or a scope for spaces of every type.
        forms.extend(words(
            "atproto transition:generic transition:chat.bsky".into(),
        ));
        forms.push(format!("space:com.example.calendar?authority={group}&skey=rides&collection=com.example.event&action=read&action=create"));
        forms.push(format!(
            "space:com.example.calendar?authority={group}&skey=rides&action=read"
        ));
        for form in &forms {
            assert!(!matches!(Scope::parse(form), Scope::Other(_)), "{form}");
            assert_eq!(upgraded(form), *form);
        }
    }
}
