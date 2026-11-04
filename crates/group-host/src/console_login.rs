//! Signing in to the console, as oneself.
//!
//! The host is a confidential OAuth client of each person's own PDS, as
//! knotless-serve is of its accounts', under a client id of its own, its
//! key derived from the host's secret. It asks for `atproto` alone: the
//! console's methods act as the group, never as the person, so the host
//! needs only to know who they are. Once their PDS says so, it revokes the
//! token it was given, and keeps nothing of theirs.
//!
//! The browser holds only a cookie the host signed, naming the person,
//! HttpOnly and SameSite=Lax. A sign-in is answered only in the browser
//! that started it, by a cookie naming its state, so a link to someone
//! else's approval cannot sign a person in as them. Signing out refuses
//! every cookie signed before.
//!
//! Every server a person's handle leads to (their PDS, its authorization
//! server) is reached only at a public address, as a client's metadata is.
//!
//! The console serves `/console/*` from the host, beside its pages.

use crate::Host;
use crate::oauth_dpop::hmac;
use crate::store::Store;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::jose;
use atproto::{Curve, Keypair, did};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// The session cookie, and the cookie naming a sign-in under way.
pub const COOKIE: &str = "group_host_session";
const STARTED: &str = "group_host_login";
/// How long a sign-in lasts, and how long approving one may take.
const LASTS: i64 = 30 * 86_400_000;
const TAKES: i64 = 15 * 60_000;

/// A sign-in gone to a PDS, waiting for the browser to come back.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub did: String,
    /// Their PDS, as their DID document named it.
    pub pds: String,
    pub issuer: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    /// The DPoP key its tokens are bound to.
    pub key: Vec<u8>,
    pub verifier: String,
    /// The page to go back to.
    pub back: String,
    pub started: i64,
    /// For a sign-in to an account being moved here, who is moving it:
    /// its tokens are kept for them, and the browser is not signed in.
    pub mover: Option<String>,
}

impl Store {
    /// Keeps a sign-in under way by its state; those lapsed by `now` are
    /// forgotten.
    pub fn start_login(&self, state: &str, p: &Pending, now: i64) -> anyhow::Result<()> {
        let conn = self.conn();
        let forgot = conn.execute(
            "DELETE FROM console_login WHERE started <= ?1",
            [now - TAKES],
        );
        forgot?;
        let values = params![
            state,
            p.did,
            p.issuer,
            p.token_endpoint,
            p.revocation_endpoint,
            p.key,
            p.verifier,
            p.back,
            p.started,
            p.mover,
            p.pds
        ];
        conn.execute(START_LOGIN, values)?;
        Ok(())
    }

    /// The sign-in `state` names, forgotten as it is read, if it began
    /// after `since`.
    pub fn finish_login(&self, state: &str, since: i64) -> anyhow::Result<Option<Pending>> {
        let conn = self.conn();
        let found = conn.query_row(FINISH_LOGIN, params![state, since], |r| {
            Ok(Pending {
                did: r.get(0)?,
                issuer: r.get(1)?,
                token_endpoint: r.get(2)?,
                revocation_endpoint: r.get(3)?,
                key: r.get(4)?,
                verifier: r.get(5)?,
                back: r.get(6)?,
                started: r.get(7)?,
                mover: r.get(8)?,
                pds: r.get(9)?,
            })
        });
        Ok(found.optional()?)
    }

    /// Notes that `did` signed out of the console at `at`.
    pub fn sign_out(&self, did: &str, at: i64) -> anyhow::Result<()> {
        self.conn().execute(SIGN_OUT, params![did, at])?;
        Ok(())
    }

    /// When `did` last signed out of the console.
    pub fn signed_out(&self, did: &str) -> anyhow::Result<Option<i64>> {
        let conn = self.conn();
        let found = conn.query_row("SELECT at FROM console_logout WHERE did = ?1", [did], |r| {
            r.get(0)
        });
        Ok(found.optional()?)
    }
}

const START_LOGIN: &str = "INSERT INTO console_login (state, did, issuer, token_endpoint, revocation_endpoint, key, verifier, back, started, mover, pds) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)";
const FINISH_LOGIN: &str = "DELETE FROM console_login WHERE state = ?1 AND started > ?2 RETURNING did, issuer, token_endpoint, revocation_endpoint, key, verifier, back, started, mover, pds";
const SIGN_OUT: &str =
    "INSERT INTO console_logout (did, at) VALUES (?1, ?2) ON CONFLICT (did) DO UPDATE SET at = ?2";

fn failed(status: StatusCode, why: impl Into<String>) -> Response {
    let why = why.into();
    tracing::info!(why, "console sign-in refused");
    (status, format!("Signing in failed: {why}")).into_response()
}

/// A cookie, as the host sets one: for the whole site, HttpOnly, Lax, and
/// Secure where the console is on https.
fn cookie(host: &Host, name: &str, value: &str, max_age: i64) -> String {
    let secure = if host.config.console_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}

/// The value of the cookie `name` a request carries.
pub fn cookie_of<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// A page of the console's to go back to: a path there, never elsewhere.
fn back_to(asked: Option<&String>) -> String {
    let path = asked.map_or("/", String::as_str);
    let ours = path.starts_with('/') && !path.starts_with("//") && !path.contains('\\');
    if ours {
        path.to_string()
    } else {
        "/".to_string()
    }
}

impl Host {
    /// The key the host signs its client assertions with, as the console.
    fn console_key(&self) -> Keypair {
        let secret = hmac(
            &self.secrets.rotation.to_bytes(),
            b"group-host console client key",
        );
        Keypair::from_bytes(Curve::P256, &secret).expect("an HMAC is a P-256 secret")
    }

    fn console_client_id(&self) -> String {
        format!("{}/console/client-metadata.json", self.config.console_url)
    }

    pub(crate) fn console_callback(&self) -> String {
        format!("{}/console/callback", self.config.console_url)
    }

    fn cookie_key(&self) -> Vec<u8> {
        hmac(
            &self.secrets.rotation.to_bytes(),
            b"group-host console cookie key",
        )
    }

    /// A session cookie's value, naming `did`, signed `now`.
    pub fn session_cookie(&self, did: &str, now: i64) -> String {
        let claims = jose::b64(json!({"did": did, "iat": now, "exp": now + LASTS}).to_string());
        format!(
            "{claims}.{}",
            jose::b64(hmac(&self.cookie_key(), claims.as_bytes()))
        )
    }

    /// Who a request's session cookie names, while it lasts and they have
    /// not signed out since it was signed.
    pub fn console_caller(&self, headers: &HeaderMap) -> anyhow::Result<Option<String>> {
        let Some((claims, sig)) = cookie_of(headers, COOKIE).and_then(|c| c.split_once('.')) else {
            return Ok(None);
        };
        if jose::b64(hmac(&self.cookie_key(), claims.as_bytes())) != sig {
            return Ok(None);
        }
        let claims = jose::json_of(claims).unwrap_or_default();
        // Signed by the host, so its claims are the host's: all there.
        let did = claims["did"].as_str().unwrap_or_default();
        let (iat, exp) = (
            claims["iat"].as_i64().unwrap_or_default(),
            claims["exp"].as_i64().unwrap_or_default(),
        );
        let out = self.store.signed_out(did)?;
        let current = exp > now_millis() && out.is_none_or(|at| iat > at);
        Ok(current.then(|| did.to_string()))
    }

    /// Sends `form` to `url` from the console's client, with a DPoP proof
    /// of `key` and its assertion for `issuer`, once more with the nonce
    /// the server asks for. The JSON answered, if it took it.
    pub(crate) async fn console_post(
        &self,
        url: &str,
        issuer: &str,
        key: &Keypair,
        form: &[(&str, &str)],
    ) -> Result<Value, String> {
        let (pinned, at) = self.pinned(url).await?;
        let mut nonce: Option<String> = None;
        for _ in 0..2 {
            let now = now_millis() / 1000;
            let mut claims =
                json!({"jti": jose::random(12), "htm": "POST", "htu": url, "iat": now});
            if let Some(n) = &nonce {
                claims["nonce"] = json!(n);
            }
            let header = json!({"typ": "dpop+jwt", "jwk": jose::jwk(&key.public())});
            let proof = jose::sign(key, &header, &claims);
            let id = self.console_client_id();
            let assertion = jose::sign(
                &self.console_key(),
                &json!({"typ": "JWT", "kid": "console"}),
                &json!({"iss": id, "sub": id, "aud": issuer, "jti": jose::random(12), "iat": now, "exp": now + 60}),
            );
            let mut fields: Vec<(&str, &str)> = form.to_vec();
            fields.extend([
                ("client_id", id.as_str()),
                (
                    "client_assertion_type",
                    crate::oauth_clients::CLIENT_ASSERTION_TYPE,
                ),
                ("client_assertion", assertion.as_str()),
            ]);
            let sent = pinned
                .post(at.clone())
                .header("dpop", proof)
                .form(&fields)
                .send()
                .await;
            let res = sent.map_err(|e| format!("{url}: {e}"))?;
            let ok = res.status().is_success();
            nonce = res
                .headers()
                .get("dpop-nonce")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            let body: Value = res.json().await.unwrap_or_default();
            if ok {
                return Ok(body);
            }
            if body["error"] != "use_dpop_nonce" {
                let said = body["error_description"]
                    .as_str()
                    .or(body["error"].as_str());
                return Err(format!(
                    "{url} refused: {}",
                    said.unwrap_or("no reason given")
                ));
            }
        }
        Err(format!("{url} asked for a nonce twice"))
    }

    /// Where `identifier`, a handle or DID, signs in: their DID, their PDS,
    /// and its authorization server's metadata.
    async fn sign_in_at(&self, identifier: &str) -> Result<(String, String, Value), String> {
        let identifier = identifier.trim().trim_start_matches('@').to_lowercase();
        let did = match identifier.starts_with("did:") {
            true => Some(identifier.clone()),
            false => {
                self.identity
                    .resolve_handle(&identifier, &self.config.handle_resolvers)
                    .await
            }
        };
        let did = did.ok_or_else(|| format!("{identifier} is no account we can find"))?;
        let doc = self
            .identity
            .document(&did)
            .await
            .map_err(|e| format!("{did}: {e:#}"))?;
        let pds = did::pds(&doc).ok_or_else(|| format!("{did} names no PDS"))?;
        let resource = self
            .public_json(
                &format!("{pds}/.well-known/oauth-protected-resource"),
                "its PDS",
            )
            .await?;
        let issuer = resource["authorization_servers"][0].as_str();
        let issuer = issuer.ok_or_else(|| format!("{pds} names no authorization server"))?;
        let server = self
            .public_json(
                &format!("{issuer}/.well-known/oauth-authorization-server"),
                "its authorization server",
            )
            .await?;
        if server["issuer"] != issuer {
            return Err(format!("{issuer} calls itself {}", server["issuer"]));
        }
        Ok((did, pds, server))
    }
}

/// The console's client metadata, at its client id.
pub async fn client_metadata(State(host): State<Arc<Host>>) -> Json<Value> {
    let mut jwk = jose::jwk(&host.console_key().public());
    jwk["kid"] = json!("console");
    jwk["use"] = json!("sig");
    jwk["alg"] = json!("ES256");
    Json(json!({
        "client_id": host.console_client_id(),
        "client_name": "Group host",
        "client_uri": host.config.console_url,
        "redirect_uris": [host.console_callback()],
        "scope": format!("atproto {}", host.move_scope()),
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "private_key_jwt",
        "token_endpoint_auth_signing_alg": "ES256",
        "application_type": "web",
        "dpop_bound_access_tokens": true,
        "jwks": {"keys": [jwk]},
    }))
}

/// Starts signing `handle` in: pushes the request to their PDS's
/// authorization server, and sends the browser there to approve it.
pub async fn login(
    State(host): State<Arc<Host>>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, XrpcError> {
    sign_in(&host, &q, "atproto", None).await
}

/// Starts a sign-in to `q`'s `handle`, asking for `scope`, for `mover`
/// where it is an account they are moving here.
pub(crate) async fn sign_in(
    host: &Host,
    q: &HashMap<String, String>,
    scope: &str,
    mover: Option<String>,
) -> Result<Response, XrpcError> {
    let Some(identifier) = q.get("handle").filter(|h| !h.trim().is_empty()) else {
        return Ok(failed(StatusCode::BAD_REQUEST, "say who you are"));
    };
    let (did, pds, server) = match host.sign_in_at(identifier).await {
        Ok(found) => found,
        Err(why) => return Ok(failed(StatusCode::BAD_REQUEST, why)),
    };
    let text = |k: &str| server[k].as_str().unwrap_or_default().to_string();
    let issuer = text("issuer");
    let key = Keypair::generate(Curve::P256);
    let (verifier, state) = (jose::random(32), jose::random(16));
    let callback = host.console_callback();
    let challenge = jose::sha256(&verifier);
    let form = [
        ("response_type", "code"),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("redirect_uri", callback.as_str()),
        ("scope", scope),
        ("state", state.as_str()),
        ("login_hint", identifier.as_str()),
    ];
    let pushed = host
        .console_post(
            &text("pushed_authorization_request_endpoint"),
            &issuer,
            &key,
            &form,
        )
        .await;
    let pushed = match pushed {
        Ok(p) => p,
        Err(why) => return Ok(failed(StatusCode::BAD_GATEWAY, why)),
    };
    let Some(request_uri) = pushed["request_uri"].as_str() else {
        return Ok(failed(
            StatusCode::BAD_GATEWAY,
            format!("{issuer} gave no request_uri"),
        ));
    };
    let Ok(mut to) = reqwest::Url::parse(&text("authorization_endpoint")) else {
        return Ok(failed(
            StatusCode::BAD_GATEWAY,
            format!("{issuer} names no authorization endpoint"),
        ));
    };
    to.query_pairs_mut()
        .append_pair("client_id", &host.console_client_id())
        .append_pair("request_uri", request_uri);
    let now = now_millis();
    let pending = Pending {
        did,
        pds,
        issuer,
        token_endpoint: text("token_endpoint"),
        revocation_endpoint: server["revocation_endpoint"].as_str().map(String::from),
        key: key.to_bytes().to_vec(),
        verifier,
        back: back_to(q.get("back")),
        started: now,
        mover,
    };
    host.store
        .start_login(&state, &pending, now)
        .map_err(XrpcError::internal)?;
    let started = cookie(host, STARTED, &state, TAKES / 1000);
    Ok(([(header::SET_COOKIE, started)], Redirect::to(to.as_str())).into_response())
}

/// The browser comes back from approving: the code is exchanged, the
/// person is who they said, and the host signs them in.
pub async fn callback(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, XrpcError> {
    let state = q.get("state").map(String::as_str).unwrap_or_default();
    let ours = cookie_of(&headers, STARTED) == Some(state);
    let since = now_millis() - TAKES;
    let pending = host
        .store
        .finish_login(state, since)
        .map_err(XrpcError::internal)?;
    let Some(p) = pending.filter(|_| ours) else {
        let why = "this browser did not start that sign-in, or it took too long";
        return Ok(failed(StatusCode::BAD_REQUEST, why));
    };
    if q.get("iss").is_some_and(|iss| *iss != p.issuer) {
        return Ok(failed(
            StatusCode::BAD_REQUEST,
            "the answer came from another server",
        ));
    }
    let Some(code) = q.get("code") else {
        let why = q.get("error_description").or(q.get("error"));
        let why = format!(
            "it was refused: {}",
            why.map_or("no reason given", String::as_str)
        );
        return Ok(failed(StatusCode::FORBIDDEN, why));
    };
    let key = Keypair::from_bytes(Curve::P256, &p.key).expect("a key the host made");
    let callback = host.console_callback();
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", callback.as_str()),
        ("code_verifier", p.verifier.as_str()),
    ];
    let tokens = match host
        .console_post(&p.token_endpoint, &p.issuer, &key, &form)
        .await
    {
        Ok(t) => t,
        Err(why) => return Ok(failed(StatusCode::BAD_GATEWAY, why)),
    };
    if tokens["sub"] != p.did.as_str() {
        let why = format!("signed in as {}, not {}", tokens["sub"], p.did);
        return Ok(failed(StatusCode::FORBIDDEN, why));
    }
    let done = cookie(&host, STARTED, "", 0);
    // An account being moved here: its tokens kept for the one moving it,
    // still signed in, and this browser signed in as nobody new.
    if let Some(mover) = &p.mover {
        let caller = host.console_caller(&headers).map_err(XrpcError::internal)?;
        if caller.as_ref() != Some(mover) {
            return Ok(failed(
                StatusCode::FORBIDDEN,
                "sign in to the console again first",
            ));
        }
        host.keep_move(&p, &tokens)
            .await
            .map_err(XrpcError::internal)?;
        tracing::info!(did = p.did, mover, "signed in to an account to move");
        return Ok(([(header::SET_COOKIE, done)], Redirect::to(&p.back)).into_response());
    }
    // Knowing who they are is all the host wanted: it keeps nothing.
    if let (Some(url), Some(token)) = (&p.revocation_endpoint, tokens["access_token"].as_str()) {
        let revoked = host
            .console_post(url, &p.issuer, &key, &[("token", token)])
            .await;
        tracing::debug!(?revoked, "gave back the console's token");
    }
    let now = now_millis();
    let session = cookie(
        &host,
        COOKIE,
        &host.session_cookie(&p.did, now),
        LASTS / 1000,
    );
    tracing::info!(did = p.did, "signed in to the console");
    let headers = AppendHeaders([(header::SET_COOKIE, session), (header::SET_COOKIE, done)]);
    Ok((headers, Redirect::to(&p.back)).into_response())
}

/// Signs out: this cookie, and every one signed before now.
pub async fn logout(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
) -> Result<Response, XrpcError> {
    let who = host.console_caller(&headers).map_err(XrpcError::internal)?;
    if let Some(did) = who {
        host.store
            .sign_out(&did, now_millis())
            .map_err(XrpcError::internal)?;
    }
    let gone = cookie(&host, COOKIE, "", 0);
    Ok(([(header::SET_COOKIE, gone)], Redirect::to("/")).into_response())
}
