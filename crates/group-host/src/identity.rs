//! Other accounts' DID documents, as the host reads them to check what
//! they sign and to find their servers: a `did:plc`'s from the PLC
//! directory, a `did:web`'s from its host, each kept a few minutes.
//!
//! The hosts identity data names, a `did:web`'s and a handle's own, are
//! held to [`crate::guard`]; the PLC directory, the addresses
//! `DID_WEB_HOSTS` sets and the handle resolvers set are asked as they are
//! set.

use crate::guard::pinned_client;
use anyhow::{Context, Error, Result, bail};
use atproto::{PublicKey, did};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a document read is taken as it stands.
pub const KEPT_FOR: Duration = Duration::from_secs(300);

/// The most bytes a document may be: a request names the host it is read
/// from, so what that host sends is a caller's to choose.
pub const DOCUMENT_AT_MOST: usize = 256 * 1024;

/// A response's body, refused once it is past [`DOCUMENT_AT_MOST`].
async fn body(mut res: reqwest::Response) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > DOCUMENT_AT_MOST {
            bail!("a DID document past {DOCUMENT_AT_MOST} bytes");
        }
    }
    Ok(bytes)
}

/// The most documents kept at once: a request names whose document is
/// read, so how many is a caller's to choose.
pub const KEPT_AT_MOST: usize = 10_000;

/// How long a lookup at a host a name alone points to may take: a
/// `did:web`'s host, or a handle's `/.well-known/atproto-did`, may be
/// anyone's, and never answer.
pub const LOOKUP: Duration = Duration::from_secs(3);

pub struct Resolver {
    plc_url: String,
    did_web_scheme: String,
    did_web_hosts: BTreeMap<String, String>,
    /// Whether a host identity data names may be asked on http, at any
    /// address, as on a network of test servers ([`crate::guard`]).
    http_here: bool,
    http: reqwest::Client,
    kept: Mutex<HashMap<String, (Instant, Value)>>,
    kept_for: Duration,
    kept_at_most: usize,
}

impl Resolver {
    pub fn new(plc_url: &str, did_web_scheme: &str) -> Resolver {
        Resolver::keeping(plc_url, did_web_scheme, KEPT_FOR, KEPT_AT_MOST)
    }

    /// A resolver keeping documents `kept_for`, at most `kept_at_most` of
    /// them at once.
    pub fn keeping(
        plc_url: &str,
        did_web_scheme: &str,
        kept_for: Duration,
        kept_at_most: usize,
    ) -> Resolver {
        Resolver {
            kept_for,
            kept_at_most,
            plc_url: plc_url.trim_end_matches('/').to_string(),
            did_web_scheme: did_web_scheme.to_string(),
            did_web_hosts: BTreeMap::new(),
            http_here: false,
            // The PLC directory and the handle resolvers are as set: one
            // that redirects is not followed elsewhere all the same.
            http: crate::client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("a client builds"),
            kept: Mutex::default(),
        }
    }

    /// This resolver, reading the documents of the `did:web`s of the hosts
    /// `hosts` names at the address it gives each instead.
    pub fn reading_at(mut self, hosts: &BTreeMap<String, String>) -> Resolver {
        self.did_web_hosts = hosts.clone();
        self
    }

    /// This resolver, for a host on http itself (`PUBLIC_URL`), as on a
    /// network of test servers: asking the hosts identity data names on
    /// http too, at any address ([`crate::guard`]).
    pub fn http_here(mut self, here: bool) -> Resolver {
        self.http_here = here;
        self
    }

    /// What a host identity data names answers at `url`, asked as
    /// [`crate::guard`] holds it.
    async fn guarded(&self, url: &str) -> Result<reqwest::Response> {
        let pinned = pinned_client(url, self.http_here).await;
        let (client, at) = pinned.map_err(Error::msg)?;
        Ok(client.get(at).timeout(LOOKUP).send().await?)
    }

    /// Where `did`'s document is read, and whether at a host the DID
    /// names, rather than one the host is set to read it at
    /// (`DID_WEB_HOSTS`) or the PLC directory.
    fn url(&self, did: &str) -> Result<(String, bool)> {
        if let Some(host) = did.strip_prefix("did:web:") {
            // A did:web of a host alone: one with a path names no host here.
            if host.is_empty() || host.contains(':') {
                bail!("{did} is not a did:web of a host");
            }
            let host = host.replace("%3A", ":");
            let (host, named) = match self.did_web_hosts.get(&host) {
                Some(set) => (set, false),
                None => (&host, true),
            };
            let scheme = &self.did_web_scheme;
            Ok((format!("{scheme}://{host}/.well-known/did.json"), named))
        } else if did.starts_with("did:plc:") {
            Ok((format!("{}/{did}", self.plc_url), false))
        } else {
            bail!("{did}: not a did:plc or a did:web");
        }
    }

    /// `did`'s document, read now, and kept.
    pub async fn document(&self, did: &str) -> Result<Value> {
        let (url, named) = self.url(did)?;
        let res = match named {
            true => self.guarded(&url).await?,
            false => self.http.get(url).send().await?,
        };
        let res = res.error_for_status().context(format!("resolving {did}"))?;
        let doc: Value = serde_json::from_slice(&body(res).await?)?;
        if doc.get("id").and_then(Value::as_str) != Some(did) {
            bail!("{did}'s document is another's");
        }
        let mut kept = self.kept.lock().expect("not poisoned");
        // Full, the lapsed go; still full, all of them.
        if kept.len() >= self.kept_at_most {
            kept.retain(|_, (at, _)| at.elapsed() < self.kept_for);
        }
        if kept.len() >= self.kept_at_most {
            kept.clear();
        }
        kept.insert(did.into(), (Instant::now(), doc.clone()));
        Ok(doc)
    }

    /// The DID `handle` names, asked of each of `resolvers` in turn
    /// (`HANDLE_RESOLVERS`): none if none of them says.
    pub async fn resolve_handle(&self, handle: &str, resolvers: &[String]) -> Option<String> {
        for resolver in resolvers {
            let named = match resolver.strip_prefix("well-known:") {
                Some(scheme) => {
                    let url = format!("{scheme}://{handle}/.well-known/atproto-did");
                    text(self.guarded(&url).await.ok()).await
                }
                None => {
                    let url = format!("{resolver}/xrpc/com.atproto.identity.resolveHandle");
                    let asked = self
                        .http
                        .get(url)
                        .query(&[("handle", handle)])
                        .timeout(LOOKUP);
                    let body = json(asked.send().await.ok()).await;
                    body.and_then(|b| b["did"].as_str().map(String::from))
                }
            };
            let named = named.map(|d| d.trim().to_string());
            if let Some(did) = named.filter(|d| d.starts_with("did:")) {
                return Some(did);
            }
        }
        None
    }

    /// `did`'s document as it was read in the last few minutes, if it
    /// was. What it names may have changed since: a key that turns a
    /// signature away is worth reading again with [`Resolver::document`].
    pub fn kept(&self, did: &str) -> Option<Value> {
        let kept = self.kept.lock().expect("not poisoned");
        let (at, doc) = kept.get(did)?;
        (at.elapsed() < self.kept_for).then(|| doc.clone())
    }

    /// What `check` makes of `did`'s signing key: with the key kept, and
    /// if that one turns it away, the key read again; with no key kept, the
    /// key read now. `None` if no key is to be had.
    pub async fn with_key<T, E>(
        &self,
        did: &str,
        check: impl Fn(&PublicKey) -> Result<T, E>,
    ) -> Option<Result<T, E>> {
        let key_in = |doc: &Value| did::signing_key(doc).ok();
        if let Some(key) = self.kept(did).as_ref().and_then(key_in) {
            let checked = check(&key);
            if checked.is_ok() {
                return Some(checked);
            }
        }
        let doc = self.document(did).await.ok()?;
        Some(check(&key_in(&doc)?))
    }
}

/// A response's text, if it answered with one no longer than a document
/// may be.
async fn text(res: Option<reqwest::Response>) -> Option<String> {
    String::from_utf8(body(res?.error_for_status().ok()?).await.ok()?).ok()
}

/// A response's JSON, as [`text`] reads it.
async fn json(res: Option<reqwest::Response>) -> Option<Value> {
    serde_json::from_slice(&body(res?.error_for_status().ok()?).await.ok()?).ok()
}
