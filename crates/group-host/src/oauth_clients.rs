//! OAuth clients: who they are, by their metadata document or, for a
//! native app on the same machine, the loopback client id
//! `http://localhost`; and how they authenticate. A public client names
//! itself; a confidential one signs a JWT with a key its metadata lists.
//!
//! Its checks follow @atproto/oauth-provider's.

use crate::Host;
use crate::index_api::capped;
use crate::oauth::OAuthError;
use atproto::jose;
use atproto_client::sender::SendError;
use reqwest::Url;
use serde_json::{Value, json};
use std::collections::HashMap;

pub const CLIENT_ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// How long a client's metadata, or key set, is kept before it is fetched
/// again.
const KEEP_FOR: i64 = 10 * 60_000;
/// How many clients' metadata is kept at once.
const KEEP_AT_MOST: usize = 1000;
/// The most bytes a metadata document or key set may be.
const DOCUMENT_AT_MOST: usize = 64 * 1024;
/// How old a client assertion may be, in seconds.
const ASSERTION_MAX_AGE: i64 = 300;

/// A client, by its id, and what its metadata says of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Client {
    pub id: String,
    pub metadata: Value,
}

/// Documents the host has read, by URL, and when.
#[derive(Default)]
struct Kept(std::sync::Mutex<HashMap<String, (i64, Value)>>);

impl Kept {
    /// The document at `url`, if read in the last few minutes.
    fn get(&self, url: &str, now: i64) -> Option<Value> {
        let kept = self.0.lock().expect("not poisoned").get(url).cloned();
        let (at, value) = kept.filter(|(at, _)| now - at < KEEP_FOR)?;
        tracing::debug!(url, at, "kept");
        Some(value)
    }

    fn keep(&self, url: &str, now: i64, value: &Value) {
        let mut kept = self.0.lock().expect("not poisoned");
        if kept.len() >= KEEP_AT_MOST {
            kept.clear();
        }
        kept.insert(url.into(), (now, value.clone()));
    }
}

/// The clients' metadata and key sets the host has read, and when.
#[derive(Default)]
pub struct Clients {
    metadata: Kept,
    keys: Kept,
}

fn invalid_metadata(description: impl Into<String>) -> OAuthError {
    OAuthError::bad("invalid_client_metadata", description)
}

fn invalid_client(description: impl Into<String>) -> OAuthError {
    OAuthError::bad("invalid_client", description)
}

fn strings(value: &Value) -> Vec<&str> {
    let items = value.as_array().map(Vec::as_slice).unwrap_or_default();
    items.iter().filter_map(Value::as_str).collect()
}

/// Whether `id` is the loopback client's: `http://localhost`, with
/// nothing but its query.
fn is_loopback(id: &str) -> bool {
    let rest = id.strip_prefix("http://localhost").unwrap_or("x");
    rest.is_empty() || rest.starts_with('?') || rest.starts_with("/?") || rest == "/"
}

/// What atproto takes the loopback client to have registered: the
/// redirects and scope its id's query names, or the loopback addresses
/// and `atproto`.
fn loopback(id: &str) -> Result<Value, OAuthError> {
    // `http://localhost` and a query: always a URL.
    let pairs: Vec<(String, String)> = Url::parse(id)
        .map(|url| url.query_pairs().into_owned().collect())
        .unwrap_or_default();
    let (mut redirects, mut scope) = (Vec::new(), Vec::new());
    for (k, v) in pairs {
        match k.as_str() {
            "redirect_uri" => redirects.push(v),
            "scope" => scope.push(v),
            _ => {
                return Err(invalid_metadata(format!(
                    "the loopback client id names {k}"
                )));
            }
        }
    }
    if scope.len() > 1 {
        return Err(invalid_metadata("the loopback client id names one scope"));
    }
    for r in &redirects {
        let at = Url::parse(r).ok();
        let host = at.as_ref().and_then(|u| u.host_str());
        let loopback = matches!(host, Some("127.0.0.1" | "[::1]"));
        if !loopback || at.is_some_and(|u| u.scheme() != "http") {
            return Err(invalid_metadata(format!(
                "a loopback client redirects to 127.0.0.1 or [::1], not {r}"
            )));
        }
    }
    if redirects.is_empty() {
        redirects = vec!["http://127.0.0.1/".into(), "http://[::1]/".into()];
    }
    Ok(json!({
        "client_id": id,
        "client_name": "An app on this device",
        "redirect_uris": redirects,
        "scope": scope.pop().unwrap_or_else(|| "atproto".into()),
        "response_types": ["code"],
        "grant_types": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_method": "none",
        "application_type": "native",
        "dpop_bound_access_tokens": true,
    }))
}

/// `metadata`, once it is what an atproto client registers.
fn validate(id: &str, metadata: Value) -> Result<Value, OAuthError> {
    let m = &metadata;
    if m["client_id"] != id {
        return Err(invalid_metadata("its client_id is not the id it is at"));
    }
    if m.get("jwks").is_some() && m.get("jwks_uri").is_some() {
        return Err(invalid_metadata("jwks or jwks_uri, not both"));
    }
    let scope = m["scope"].as_str().unwrap_or_default();
    if !scope.split(' ').any(|s| s == "atproto") {
        return Err(invalid_metadata("its scope has no atproto"));
    }
    let grants = strings(&m["grant_types"]);
    if let Some(g) = grants
        .iter()
        .find(|g| !["authorization_code", "refresh_token"].contains(g))
    {
        return Err(invalid_metadata(format!(
            "the grant type {g} is not one here"
        )));
    }
    if !strings(&m["response_types"]).contains(&"code") || !grants.contains(&"authorization_code") {
        return Err(invalid_metadata("a client takes a code"));
    }
    if m["dpop_bound_access_tokens"] != true {
        return Err(invalid_metadata("dpop_bound_access_tokens is true"));
    }
    if strings(&m["redirect_uris"]).is_empty() {
        return Err(invalid_metadata("a client has a redirect_uri"));
    }
    let signing = m.get("token_endpoint_auth_signing_alg");
    match m["token_endpoint_auth_method"].as_str() {
        Some("none") if signing.is_some() => Err(invalid_metadata(
            "a public client has no token_endpoint_auth_signing_alg",
        )),
        Some("none") => Ok(metadata),
        Some("private_key_jwt") if m.get("jwks").is_none() && m.get("jwks_uri").is_none() => {
            Err(invalid_metadata("private_key_jwt needs jwks or jwks_uri"))
        }
        Some("private_key_jwt") if signing.is_none() => Err(invalid_metadata(
            "private_key_jwt needs token_endpoint_auth_signing_alg",
        )),
        Some("private_key_jwt") if m["application_type"] == "native" => {
            Err(invalid_metadata("a native client is a public one"))
        }
        Some("private_key_jwt") => Ok(metadata),
        other => Err(invalid_metadata(format!(
            "the token_endpoint_auth_method {other:?} is not one here"
        ))),
    }
}

/// Whether a later request authenticates as the first did: the same
/// method, and for a confidential client, the same key, by its thumbprint.
pub fn same_auth(first: &Value, now: &Value) -> bool {
    first["method"] == now["method"] && first["jkt"] == now["jkt"]
}

/// Whether a client registered the redirect `uri`: exactly, or at a
/// loopback address on whatever port the app listens on now (RFC 8252
/// § 7.3), as a native app registers it without one.
pub fn registered(uris: &[&str], uri: &str) -> bool {
    if uris.contains(&uri) {
        return true;
    }
    let Ok(mut at) = Url::parse(uri) else {
        return false;
    };
    let loopback = matches!(at.host_str(), Some("127.0.0.1" | "[::1]"));
    if at.scheme() != "http" || !loopback {
        return false;
    }
    let _ = at.set_port(None);
    uris.contains(&at.as_str())
}

/// The thumbprint of the key of `keys`, by `kid` if it names one, that
/// signed `assertion`, and its claims.
fn signed_by(keys: &Value, kid: &str, assertion: &str) -> Option<(String, Value)> {
    let empty = Vec::new();
    let keys = keys["keys"].as_array().unwrap_or(&empty);
    keys.iter()
        .filter(|k| kid.is_empty() || k["kid"] == kid)
        .filter_map(|k| jose::from_jwk(k).ok())
        .find_map(|k| Some((jose::thumbprint(&k), jose::verify(assertion, &k).ok()?.1)))
}

impl Host {
    /// The client `id` names, its metadata fetched and checked, or kept
    /// from a fetch in the last few minutes.
    pub async fn client(&self, id: &str, now: i64) -> Result<Client, OAuthError> {
        if let Some(metadata) = self.clients.metadata.get(id, now) {
            return Ok(Client {
                id: id.into(),
                metadata,
            });
        }
        let metadata = if is_loopback(id) {
            loopback(id)?
        } else {
            validate(id, self.fetch_json(id, "client metadata").await?)?
        };
        self.clients.metadata.keep(id, now, &metadata);
        Ok(Client {
            id: id.into(),
            metadata,
        })
    }

    /// The JSON object at `url`, asked as any host others' data names is
    /// ([`Host::guarded`]).
    pub(crate) async fn public_json(&self, url: &str, what: &str) -> Result<Value, String> {
        let unreadable = |why: String| format!("{what} at {url}: {why}");
        let asked = self.guarded.get(url).map_err(|e| unreadable(e.0))?;
        let sent = asked.header("accept", "application/json");
        // With its causes: a name the guard would not resolve says so there.
        let failed = |e: SendError| {
            let causes = anyhow::Error::new(e);
            let causes: Vec<String> = causes.chain().skip(1).map(ToString::to_string).collect();
            unreadable(causes.join(": "))
        };
        let res = sent.send().await.map_err(failed)?;
        if !res.status().is_success() {
            return Err(unreadable(format!("answered {}", res.status())));
        }
        let bytes = capped(res, DOCUMENT_AT_MOST).await;
        let bytes = bytes.ok_or_else(|| unreadable("too large".into()))?;
        let json = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object);
        json.ok_or_else(|| unreadable("not a JSON object".into()))
    }

    async fn fetch_json(&self, url: &str, what: &str) -> Result<Value, OAuthError> {
        self.public_json(url, what).await.map_err(invalid_metadata)
    }

    /// The key set at `uri`, kept from a fetch in the last few minutes
    /// unless asked `again`, and whether it was kept.
    async fn client_keys(
        &self,
        uri: &str,
        now: i64,
        again: bool,
    ) -> Result<(Value, bool), OAuthError> {
        if let Some(keys) = self.clients.keys.get(uri, now).filter(|_| !again) {
            return Ok((keys, true));
        }
        let keys = self.fetch_json(uri, "the client's keys").await?;
        self.clients.keys.keep(uri, now, &keys);
        Ok((keys, false))
    }

    /// How `client` authenticates on a request whose form is `form`: as
    /// itself alone, if public; with an assertion signed by one of its
    /// keys, for this host, if confidential.
    pub async fn authenticate_client(
        &self,
        client: &Client,
        form: &HashMap<String, String>,
        now: i64,
    ) -> Result<Value, OAuthError> {
        let assertion = form.get("client_assertion");
        if client.metadata["token_endpoint_auth_method"] == "none" {
            if assertion.is_some() {
                return Err(invalid_client("a public client sends no client assertion"));
            }
            return Ok(json!({"method": "none"}));
        }
        let typed = form.get("client_assertion_type").map(String::as_str);
        let (Some(assertion), Some(CLIENT_ASSERTION_TYPE)) = (assertion, typed) else {
            return Err(invalid_client(
                "this client authenticates with private_key_jwt",
            ));
        };
        let (header, _) = jose::parts(assertion).map_err(|e| invalid_client(e.0))?;
        let kid = header["kid"].as_str().unwrap_or_default();
        let uri = client.metadata["jwks_uri"].as_str().unwrap_or_default();
        let (keys, kept) = match client.metadata.get("jwks") {
            Some(jwks) => (jwks.clone(), false),
            None => self.client_keys(uri, now, false).await?,
        };
        let mut verified = signed_by(&keys, kid, assertion);
        if verified.is_none() && kept {
            // Keys rotate: one not among those kept has them fetched again.
            let (keys, _) = self.client_keys(uri, now, true).await?;
            verified = signed_by(&keys, kid, assertion);
        }
        let verified = verified.ok_or_else(|| invalid_client("no key of the client signed this"));
        let (jkt, claims) = verified?;
        let issuer = &self.config.public_url;
        let audiences = [
            issuer.clone(),
            format!("{issuer}/oauth/token"),
            format!("{issuer}/oauth/par"),
        ];
        let for_us = match &claims["aud"] {
            Value::String(aud) => audiences.contains(aud),
            aud => strings(aud)
                .iter()
                .any(|a| audiences.iter().any(|b| b == a)),
        };
        if claims["iss"] != client.id.as_str() || claims["sub"] != client.id.as_str() || !for_us {
            return Err(invalid_client(
                "a client assertion is the client's own, for this host",
            ));
        }
        let secs = now / 1000;
        let exp = claims["exp"].as_i64().unwrap_or(i64::MAX);
        let iat = claims["iat"].as_i64().unwrap_or_default();
        if exp <= secs || iat < secs - ASSERTION_MAX_AGE || iat > secs + 10 {
            return Err(invalid_client(
                "a client assertion too old, or from the future",
            ));
        }
        let jti = claims["jti"].as_str().filter(|j| !j.is_empty());
        let jti = jti.ok_or_else(|| invalid_client("a client assertion without a jti"))?;
        let until = exp.min(secs + ASSERTION_MAX_AGE) * 1000;
        let fresh = self
            .store
            .spend(&format!("client:{}", client.id), jti, until, now);
        if !fresh.map_err(OAuthError::internal)? {
            return Err(invalid_client("a client assertion used before"));
        }
        Ok(json!({"method": "private_key_jwt", "jkt": jkt}))
    }
}
