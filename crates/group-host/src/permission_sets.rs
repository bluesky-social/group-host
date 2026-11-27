//! Other apps' lexicons: their permission sets, as an app signing in as a
//! group asks for one
//! (`include:social.grain.authFull?aud=did:web:grain.social%23grain_appview`),
//! and what a kind of record an app wrote as the group is, as its lexicon
//! describes it. Each is read as atproto publishes lexicons: the TXT
//! record `_lexicon.<authority>` (the NSID's segments but the last,
//! reversed) names a DID, from `DNS_OVER_HTTPS`, and that DID's repo, at
//! its PDS, holds the lexicon as the record
//! `com.atproto.lexicon.schema/<nsid>`. A lexicon read, or found missing,
//! is kept ten minutes, for both.
//!
//! What an include gives is what atproto's permissions give of one: each
//! `repo` permission's collections and each `rpc` permission's methods,
//! all within the set's own namespace (`social.grain.` of
//! `social.grain.authFull`), an `rpc` one's audience the include's where
//! it inherits it (`inheritAud`), and none that names an audience but
//! `*`. The rest of a set, its `space` permissions among it, gives a
//! group's session nothing.

use crate::Host;
use crate::rbac::NS;
use atproto::did;
use atproto::scope::Scope;
use atproto::syntax::is_did;
use reqwest::Url;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

/// How long a lexicon read, or found missing, is kept.
const KEEP_FOR: i64 = 10 * 60_000;
/// How many lexicons are kept at once.
const KEEP_AT_MOST: usize = 1000;
/// The collection a lexicon is published in.
const SCHEMA: &str = "com.atproto.lexicon.schema";
/// The DNS type of a TXT record.
const TXT: u64 = 16;

/// The sets an app asked for, each its lexicon's main def, by the
/// `include:` scope that asked for it.
pub type Sets = BTreeMap<String, Value>;

/// The lexicons read, by NSID, and when: of each, its main def (`main`)
/// and its own description (`description`), or `None` for one that did
/// not resolve.
#[derive(Default)]
pub struct Read(Mutex<HashMap<String, (i64, Option<Value>)>>);

/// The NSID an `include:` scope names, and its audience, if it is one of
/// another app's sets.
pub fn include(scope: &str) -> Option<(String, Option<String>)> {
    match Scope::parse(scope) {
        Scope::Include(i) if !i.nsid.starts_with(&format!("{NS}.")) => Some((i.nsid, i.aud)),
        _ => None,
    }
}

/// Whether `nsid` is within the namespace of the set `set`: its NSID up
/// to its last dot, and more.
fn within(set: &str, nsid: &str) -> bool {
    let space = &set[..=set.rfind('.').expect("an NSID has dots")];
    nsid.len() > space.len() && nsid.starts_with(space)
}

/// The strings of `value`, an array.
fn strings(value: &Value) -> Vec<&str> {
    let items = value.as_array().map(Vec::as_slice).unwrap_or_default();
    items.iter().filter_map(Value::as_str).collect()
}

/// The scopes the `include:` scope `scope` gives of `set`, its main def,
/// as atproto reads one: a scope for each collection or method.
pub fn included(scope: &str, set: &Value) -> Vec<String> {
    let (nsid, aud) = include(scope).expect("an include of another app's set");
    let mut out = Vec::new();
    for p in set["permissions"].as_array().into_iter().flatten() {
        let (resource, names) = match p["resource"].as_str() {
            Some("repo") => ("repo", strings(&p["collection"])),
            Some("rpc") => ("rpc", strings(&p["lxm"])),
            _ => continue,
        };
        if names.is_empty() || !names.iter().all(|n| within(&nsid, n)) {
            continue;
        }
        let query = match (resource, p["aud"].as_str(), &aud) {
            ("repo", ..) => strings(&p["action"])
                .iter()
                .map(|a| format!("action={a}"))
                .collect::<Vec<_>>()
                .join("&"),
            (_, Some("*"), _) => "aud=*".into(),
            (_, None, Some(aud)) if p["inheritAud"] == true => {
                format!("aud={}", aud.replace('#', "%23"))
            }
            _ => continue,
        };
        let at = |n: &&str| match query.is_empty() {
            true => format!("{resource}:{n}"),
            false => format!("{resource}:{n}?{query}"),
        };
        out.extend(names.iter().map(at));
    }
    out
}

/// The text of a TXT record's data as DNS-over-HTTPS's JSON gives it:
/// its quoted strings, joined.
fn txt(data: &str) -> String {
    data.split('"').skip(1).step_by(2).collect()
}

impl Read {
    fn get(&self, nsid: &str, now: i64) -> Option<Option<Value>> {
        let kept = self.0.lock().expect("not poisoned");
        let (at, lexicon) = kept.get(nsid).filter(|(at, _)| now - at < KEEP_FOR)?;
        tracing::debug!(nsid, at, "kept");
        Some(lexicon.clone())
    }

    fn keep(&self, nsid: &str, now: i64, lexicon: Option<Value>) {
        let mut kept = self.0.lock().expect("not poisoned");
        // Full, all of them go.
        let full = kept.len() >= KEEP_AT_MOST;
        kept.retain(|_, _| !full);
        kept.insert(nsid.into(), (now, lexicon));
    }
}

impl Host {
    /// The sets of other apps `requested` asks for that resolve.
    pub async fn permission_sets(&self, requested: &[String]) -> Sets {
        let mut sets = Sets::new();
        for scope in requested {
            let Some((nsid, _)) = include(scope) else {
                continue;
            };
            if let Some(set) = self.permission_set(&nsid).await {
                sets.insert(scope.clone(), set);
            }
        }
        sets
    }

    /// The set `nsid`, its lexicon's main def, if it is one.
    async fn permission_set(&self, nsid: &str) -> Option<Value> {
        let lexicon = self.lexicon(nsid).await?;
        let set = lexicon["main"]["type"] == "permission-set";
        set.then(|| lexicon["main"].clone())
    }

    /// What the kind of record `collection`, another app's, is, as its
    /// lexicon says: its main def's description, or else the lexicon's
    /// own. None for one of this host's.
    pub async fn lexicon_description(&self, collection: &str) -> Option<String> {
        let theirs = Some(collection).filter(|c| !c.starts_with(&format!("{NS}.")));
        let lexicon = self.lexicon(theirs?).await?;
        let main = lexicon["main"]["description"].as_str();
        let said = main.or(lexicon["description"].as_str())?;
        Some(said.into())
    }

    /// The lexicon `nsid`, kept or read now.
    async fn lexicon(&self, nsid: &str) -> Option<Value> {
        let now = crate::time::now_millis();
        if let Some(kept) = self.lexicons.get(nsid, now) {
            return kept;
        }
        let lexicon = self.read_lexicon(nsid).await;
        tracing::info!(nsid, resolved = lexicon.is_some(), "read a lexicon");
        self.lexicons.keep(nsid, now, lexicon.clone());
        lexicon
    }

    /// The lexicon `nsid`, read from its publisher's repo: its main def
    /// and its description, all that is kept of it.
    async fn read_lexicon(&self, nsid: &str) -> Option<Value> {
        let publisher = self.lexicon_did(nsid).await?;
        let doc = self.identity.document_now(&publisher).await.ok()?;
        let pds = did::pds(&doc)?;
        let at = format!("{pds}/xrpc/com.atproto.repo.getRecord");
        let q = [
            ("repo", publisher.as_str()),
            ("collection", SCHEMA),
            ("rkey", nsid),
        ];
        let url = Url::parse_with_params(&at, q).ok()?;
        let record = self.public_json(url.as_str(), "a lexicon").await.ok()?;
        let lexicon = &record["value"];
        let kept = json!({"main": lexicon["defs"]["main"], "description": lexicon["description"]});
        (lexicon["id"] == nsid).then_some(kept)
    }

    /// The DID the `_lexicon` TXT record of `nsid`'s authority names: one
    /// record alone may.
    async fn lexicon_did(&self, nsid: &str) -> Option<String> {
        let mut authority: Vec<&str> = nsid.split('.').collect();
        authority.pop();
        authority.reverse();
        let name = format!("_lexicon.{}", authority.join("."));
        let asked = self.http.get(&self.config.dns_over_https);
        let asked = asked.query(&[("name", name.as_str()), ("type", "TXT")]);
        let res = asked.header("accept", "application/dns-json").send().await;
        let answer: Value = res.ok()?.json().await.ok()?;
        let records = answer["Answer"].as_array()?.iter();
        let texts = records
            .filter(|r| r["type"] == json!(TXT))
            .map(|r| txt(r["data"].as_str().unwrap_or_default()));
        let mut dids: Vec<String> = texts
            .filter_map(|t| t.strip_prefix("did=").map(String::from))
            .collect();
        (dids.len() == 1 && is_did(&dids[0])).then(|| dids.remove(0))
    }
}
