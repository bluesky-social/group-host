//! The OAuth authorization server apps sign in to a group with, as atproto
//! profiles OAuth: pushed requests, PKCE, DPoP-bound tokens, clients
//! known by their metadata documents.
//!
//! A group has no password: whoever signs in as one is a member, acting
//! with their roles. So there is no sign-in form here. The authorization
//! endpoint sends the browser to the console, where the member, signed
//! in as themselves, picks the group and approves; the console tells this
//! host so with `fyi.opensocial.internal.approveAuthorization` (`authorization.rs`).
//!
//! Tokens are opaque and kept, as their hashes, so a grant ends the moment
//! it is revoked or its member is put out of the group.

use crate::Host;
use crate::oauth_clients::{registered, same_auth};
use crate::oauth_store::{Grant, Request, Tokens};
use crate::time::now_millis;
use atproto::jose;
use axum::Json;
use axum::extract::rejection::FormRejection;
use axum::extract::{Form, Query, Request as HttpRequest, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

pub const REQUEST_URI_PREFIX: &str = "urn:ietf:params:oauth:request_uri:";

/// How long a pushed request waits on a member, and a code on its client.
const REQUEST_TTL: i64 = 5 * 60_000;
const CODE_TTL: i64 = 60_000;
/// How long an access token lasts, and a grant without a refresh: a
/// public client's two weeks, a confidential one's three months.
const ACCESS_TTL: i64 = 60 * 60_000;
const PUBLIC_TTL: i64 = 14 * 86_400_000;
const CONFIDENTIAL_TTL: i64 = 90 * 86_400_000;

/// An OAuth failure, as the authorization server's endpoints answer it:
/// `{error, error_description}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthError {
    pub status: StatusCode,
    pub error: &'static str,
    pub description: String,
}

impl OAuthError {
    /// A request refused: 400 and why.
    pub fn bad(error: &'static str, description: impl Into<String>) -> OAuthError {
        OAuthError {
            status: StatusCode::BAD_REQUEST,
            error,
            description: description.into(),
        }
    }

    pub fn internal(err: impl std::fmt::Display) -> OAuthError {
        OAuthError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: "server_error",
            description: err.to_string(),
        }
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let body = json!({"error": self.error, "error_description": self.description});
        (
            self.status,
            [(header::CACHE_CONTROL, "no-store")],
            Json(body),
        )
            .into_response()
    }
}

fn invalid_request(description: impl Into<String>) -> OAuthError {
    OAuthError::bad("invalid_request", description)
}

fn invalid_grant(description: impl Into<String>) -> OAuthError {
    OAuthError::bad("invalid_grant", description)
}

/// An answer from the authorization server: never kept, and with the
/// nonce for the client's next proof.
fn answer(host: &Host, status: StatusCode, body: Value) -> Response {
    let nonce = host.dpop_nonce(now_millis());
    let headers = [
        (header::CACHE_CONTROL, "no-store".to_string()),
        (header::HeaderName::from_static("dpop-nonce"), nonce),
    ];
    (status, headers, Json(body)).into_response()
}

fn failed(host: &Host, e: OAuthError) -> Response {
    tracing::info!(error = e.error, description = %e.description, "oauth refused");
    let mut res = e.into_response();
    let nonce = host
        .dpop_nonce(now_millis())
        .parse()
        .expect("base64url is a header value");
    res.headers_mut().insert("dpop-nonce", nonce);
    res
}

/// Lets a page anywhere reach the authorization server, as an app in a
/// browser does: its requests carry no cookies, so any origin may.
pub async fn cors(req: HttpRequest, next: Next) -> Response {
    let mut res = if req.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(req).await
    };
    let headers = res.headers_mut();
    for (k, v) in [
        (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS"),
        (
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "content-type, dpop, authorization",
        ),
        (
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "dpop-nonce, www-authenticate, retry-after, ratelimit-limit, ratelimit-remaining, ratelimit-reset, ratelimit-policy",
        ),
        (header::ACCESS_CONTROL_MAX_AGE, "600"),
    ] {
        headers.insert(k, HeaderValue::from_static(v));
    }
    res
}

/// What the authorization server is, for a client to find its endpoints.
pub async fn server_metadata(State(host): State<Arc<Host>>) -> Json<Value> {
    let base = &host.config.public_url;
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "pushed_authorization_request_endpoint": format!("{base}/oauth/par"),
        "revocation_endpoint": format!("{base}/oauth/revoke"),
        "require_pushed_authorization_requests": true,
        "scopes_supported": ["atproto"],
        "subject_types_supported": ["public"],
        "response_types_supported": ["code"],
        "response_modes_supported": ["query", "fragment"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "ui_locales_supported": ["en-US"],
        "display_values_supported": ["page"],
        "request_parameter_supported": false,
        "request_uri_parameter_supported": true,
        "require_request_uri_registration": true,
        "authorization_response_iss_parameter_supported": true,
        "token_endpoint_auth_methods_supported": ["none", "private_key_jwt"],
        "token_endpoint_auth_signing_alg_values_supported": ["ES256", "ES256K"],
        "dpop_signing_alg_values_supported": ["ES256", "ES256K"],
        "client_id_metadata_document_supported": true,
        "protected_resources": [base],
    }))
}

/// What this host is as a resource server: its own authorization server's.
pub async fn resource_metadata(State(host): State<Arc<Host>>) -> Json<Value> {
    let base = &host.config.public_url;
    Json(json!({
        "resource": base,
        "authorization_servers": [base],
        "scopes_supported": [],
        "bearer_methods_supported": ["header"],
        "resource_documentation": "https://atproto.com",
    }))
}

type OAuthForm = Result<Form<HashMap<String, String>>, FormRejection>;

fn form_of(form: OAuthForm) -> Result<HashMap<String, String>, OAuthError> {
    let Form(form) = form.map_err(|e| invalid_request(e.body_text()))?;
    Ok(form)
}

fn need<'a>(form: &'a HashMap<String, String>, name: &str) -> Result<&'a str, OAuthError> {
    let value = form.get(name).map(String::as_str).filter(|v| !v.is_empty());
    value.ok_or_else(|| invalid_request(format!("{name} is required")))
}

/// The client a form names, authenticated, and the DPoP key its request
/// is proved with: what both endpoints ask first.
async fn client_and_key(
    host: &Host,
    headers: &HeaderMap,
    form: &HashMap<String, String>,
    path: &str,
    now: i64,
) -> Result<(crate::oauth_clients::Client, Value, String), OAuthError> {
    let client_id = need(form, "client_id")?;
    let proof = host.dpop_proof(headers, "POST", path, None, now)?;
    let proof =
        proof.ok_or_else(|| OAuthError::bad("invalid_dpop_proof", "DPoP proof required"))?;
    let client = host.client(client_id, now).await?;
    let auth = host.authenticate_client(&client, form, now).await?;
    Ok((client, auth, proof.jkt))
}

/// A client pushes what it asks for, before it sends the browser here.
pub async fn par(State(host): State<Arc<Host>>, headers: HeaderMap, form: OAuthForm) -> Response {
    match push(&host, &headers, form).await {
        Ok(pushed) => answer(&host, StatusCode::CREATED, pushed),
        Err(e) => failed(&host, e),
    }
}

async fn push(host: &Host, headers: &HeaderMap, form: OAuthForm) -> Result<Value, OAuthError> {
    let form = form_of(form)?;
    let now = now_millis();
    let (client, auth, jkt) = client_and_key(host, headers, &form, "/oauth/par", now).await?;
    if form.contains_key("request") {
        return Err(OAuthError::bad(
            "request_not_supported",
            "request objects are not taken here",
        ));
    }
    if form.get("response_type").map(String::as_str) != Some("code") {
        return Err(OAuthError::bad(
            "unsupported_response_type",
            "response_type is code",
        ));
    }
    let uris: Vec<&str> = client.metadata["redirect_uris"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let only = (uris.len() == 1).then(|| uris[0]);
    let redirect = form.get("redirect_uri").map(String::as_str).or(only);
    let redirect = redirect.filter(|r| registered(&uris, r));
    let redirect =
        redirect.ok_or_else(|| invalid_request("redirect_uri is not one the client registered"))?;
    let challenge = form.get("code_challenge").filter(|c| !c.is_empty());
    let s256 = form.get("code_challenge_method").map(String::as_str) == Some("S256");
    let challenge = challenge.filter(|_| s256);
    let challenge = challenge.ok_or_else(|| invalid_request("PKCE with S256 is required"))?;
    let mode = form.get("response_mode").map_or("query", String::as_str);
    if !["query", "fragment"].contains(&mode) {
        return Err(invalid_request(format!(
            "the response_mode {mode} is not one here"
        )));
    }
    if form.get("dpop_jkt").is_some_and(|j| *j != jkt) {
        return Err(OAuthError::bad(
            "invalid_dpop_key_binding",
            "dpop_jkt is not the DPoP proof's key",
        ));
    }
    let declared = client.metadata["scope"].as_str().unwrap_or_default();
    let asked = form.get("scope").map_or(declared, String::as_str);
    let scope: Vec<&str> = asked.split(' ').filter(|s| !s.is_empty()).collect();
    if !scope.contains(&"atproto") {
        return Err(OAuthError::bad(
            "invalid_scope",
            "the atproto scope is required",
        ));
    }
    let undeclared: Vec<&str> = scope
        .iter()
        .filter(|s| !declared.split(' ').any(|d| d == **s))
        .copied()
        .collect();
    if !undeclared.is_empty() {
        let said = format!("not in the client's metadata: {}", undeclared.join(" "));
        return Err(OAuthError::bad("invalid_scope", said));
    }
    let mut parameters = json!({
        "redirect_uri": redirect,
        "scope": scope.join(" "),
        "code_challenge": challenge,
        "response_mode": mode,
        "dpop_jkt": jkt,
    });
    for key in ["state", "login_hint"] {
        if let Some(v) = form.get(key) {
            parameters[key] = json!(v);
        }
    }
    let id = format!("req-{}", jose::random(16));
    let request = Request {
        id: id.clone(),
        client_id: client.id,
        client_auth: auth,
        parameters,
        expires_at: now + REQUEST_TTL,
        group: None,
        member: None,
        scope: None,
    };
    host.store
        .push_request(&request, now)
        .map_err(OAuthError::internal)?;
    Ok(json!({
        "request_uri": format!("{REQUEST_URI_PREFIX}{id}"),
        "expires_in": REQUEST_TTL / 1000 - 1,
    }))
}

impl Host {
    /// The request `request_uri` names, while it waits on a member.
    pub fn pending_request(&self, request_uri: &str) -> anyhow::Result<Option<Request>> {
        let Some(id) = request_uri.strip_prefix(REQUEST_URI_PREFIX) else {
            return Ok(None);
        };
        self.store.pending_request(id, now_millis())
    }

    /// The URL the browser goes back to the client with: a code, or an
    /// error, its state, and this issuer.
    fn redirect_to(&self, request: &Request, values: &[(&str, &str)]) -> String {
        let p = &request.parameters;
        let mut url = reqwest::Url::parse(p["redirect_uri"].as_str().unwrap_or_default())
            .expect("a redirect the client registered");
        let mut pairs = reqwest::Url::parse("http://x/").expect("a URL");
        {
            let mut q = pairs.query_pairs_mut();
            q.extend_pairs(values);
            if let Some(state) = p["state"].as_str() {
                q.append_pair("state", state);
            }
            q.append_pair("iss", &self.config.public_url);
        }
        let query = pairs.query().unwrap_or_default();
        if p["response_mode"] == "fragment" {
            url.set_fragment(Some(query));
        } else {
            let mut q = url.query_pairs_mut();
            q.extend_pairs(pairs.query_pairs());
        }
        url.to_string()
    }

    /// A member approved `request` for `group` with `scope`: binds it and
    /// hands back where the browser goes, with its code. `None` if it
    /// waits no longer.
    pub fn approve_request(
        &self,
        request: &Request,
        group: &str,
        member: &str,
        scope: &str,
    ) -> anyhow::Result<Option<String>> {
        let now = now_millis();
        let code = format!("cod-{}", jose::random(24));
        let approved = Request {
            group: Some(group.into()),
            member: Some(member.into()),
            scope: Some(scope.into()),
            expires_at: now + CODE_TTL,
            ..request.clone()
        };
        let approved = self
            .store
            .approve_request(&request.id, &approved, &code, now)?;
        Ok(approved.then(|| self.redirect_to(request, &[("code", &code)])))
    }

    /// A member declined `request`: forgets it, and hands back where the
    /// browser goes, with access_denied. `None` if it waits no longer.
    pub fn decline_request(&self, request: &Request) -> anyhow::Result<Option<String>> {
        let dropped = self.store.drop_request(&request.id, now_millis())?;
        let values = [
            ("error", "access_denied"),
            ("error_description", "the request was declined"),
        ];
        Ok(dropped.then(|| self.redirect_to(request, &values)))
    }
}

/// The browser comes here from the client, and goes on to the console to
/// approve.
pub async fn authorize(
    State(host): State<Arc<Host>>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, OAuthError> {
    let uri = q.get("request_uri").map_or("", String::as_str);
    let pending = host.pending_request(uri).map_err(OAuthError::internal)?;
    let Some(_) = pending.filter(|p| q.get("client_id") == Some(&p.client_id)) else {
        let gone =
            "This sign-in request is unknown or has expired. Go back to the app and try again.";
        return Ok((StatusCode::BAD_REQUEST, gone).into_response());
    };
    let console = format!("{}/authorize", host.config.console_url);
    let mut to = reqwest::Url::parse(&console).expect("CONSOLE_URL is a URL");
    to.query_pairs_mut().append_pair("request_uri", uri);
    Ok(Redirect::to(to.as_str()).into_response())
}

/// A code or refresh token, exchanged for tokens.
pub async fn token(State(host): State<Arc<Host>>, headers: HeaderMap, form: OAuthForm) -> Response {
    match exchange(&host, &headers, form).await {
        Ok(tokens) => answer(&host, StatusCode::OK, tokens),
        Err(e) => failed(&host, e),
    }
}

/// How long a grant lasts from `now` without a refresh.
fn lasts(auth: &Value, now: i64) -> i64 {
    now + if auth["method"] == "none" {
        PUBLIC_TTL
    } else {
        CONFIDENTIAL_TTL
    }
}

async fn exchange(host: &Host, headers: &HeaderMap, form: OAuthForm) -> Result<Value, OAuthError> {
    let form = form_of(form)?;
    let now = now_millis();
    let (client, auth, jkt) = client_and_key(host, headers, &form, "/oauth/token", now).await?;
    let (access, refresh, access_expires, expires_at) = new_tokens(&auth, now);
    let tokens = Tokens {
        access_hash: &jose::sha256(&access),
        access_expires,
        refresh_hash: &jose::sha256(&refresh),
        expires_at,
    };
    let grant = match form.get("grant_type").map(String::as_str) {
        Some("authorization_code") => {
            let code = need(&form, "code")?;
            let redeemed = host.store.redeem_code(code).map_err(OAuthError::internal)?;
            let request = redeemed.ok_or_else(|| invalid_grant("an unknown code"))?;
            let p = &request.parameters;
            if request.expires_at <= now {
                return Err(invalid_grant("the code has lapsed"));
            }
            if request.client_id != client.id {
                return Err(invalid_grant("the code was not issued to this client"));
            }
            if form.get("redirect_uri").map(String::as_str) != p["redirect_uri"].as_str() {
                return Err(invalid_grant("redirect_uri is not the request's"));
            }
            // As RFC 7636 has one: 43 to 128 unreserved characters.
            let verifier = form.get("code_verifier").filter(|v| {
                (43..=128).contains(&v.len())
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
            });
            let verifier = verifier.map_or(String::new(), |v| jose::sha256(v));
            if p["code_challenge"] != verifier {
                return Err(invalid_grant("the code_verifier is not the challenge's"));
            }
            if p["dpop_jkt"] != jkt {
                return Err(OAuthError::bad(
                    "invalid_dpop_key_binding",
                    "the DPoP key is not the request's",
                ));
            }
            if !same_auth(&request.client_auth, &auth) {
                return Err(invalid_grant(
                    "the client authenticates otherwise than it did",
                ));
            }
            let grant = Grant {
                id: format!("tok-{}", jose::random(16)),
                group: request.group.unwrap_or_default(),
                member: request.member.unwrap_or_default(),
                client_id: client.id,
                client_auth: auth,
                scope: request.scope.unwrap_or_default(),
                dpop_jkt: jkt,
                access_expires: tokens.access_expires,
                expires_at: tokens.expires_at,
                created_at: now,
                source: "oauth".into(),
                used_at: None,
            };
            host.issue_grant(&grant, &tokens)?;
            grant
        }
        Some("refresh_token") => {
            let old = jose::sha256(need(&form, "refresh_token")?);
            let found = host
                .store
                .grant_to_refresh(&old, now)
                .map_err(OAuthError::internal)?;
            let grant = found.ok_or_else(|| invalid_grant("an unknown or lapsed refresh token"))?;
            if grant.client_id != client.id {
                return Err(invalid_grant("the token was not issued to this client"));
            }
            if grant.dpop_jkt != jkt {
                return Err(OAuthError::bad(
                    "invalid_dpop_key_binding",
                    "the DPoP key is not the token's",
                ));
            }
            if !same_auth(&grant.client_auth, &auth) {
                return Err(invalid_grant(
                    "the client authenticates otherwise than it did",
                ));
            }
            let rotated = host
                .store
                .rotate(&old, &tokens, now)
                .map_err(OAuthError::internal)?;
            // Refreshed at once by another request, which took it.
            let fresh = rotated
                .then_some(grant)
                .ok_or(invalid_grant("the refresh token was used"));
            fresh?
        }
        other => {
            return Err(OAuthError::bad(
                "unsupported_grant_type",
                format!("the grant_type {other:?} is not one here"),
            ));
        }
    };
    Ok(token_response(&access, &refresh, &grant))
}

/// A token response, as `/oauth/token` answers one.
pub(crate) fn token_response(access: &str, refresh: &str, grant: &Grant) -> Value {
    json!({
        "access_token": access,
        "token_type": "DPoP",
        "expires_in": ACCESS_TTL / 1000,
        "refresh_token": refresh,
        "scope": grant.scope,
        "sub": grant.group,
    })
}

/// New tokens for a grant made `now` with client authentication `auth`:
/// the tokens, and their hashes and lapses to keep.
pub(crate) fn new_tokens(auth: &Value, now: i64) -> (String, String, i64, i64) {
    (
        jose::random(32),
        jose::random(32),
        now + ACCESS_TTL,
        lasts(auth, now),
    )
}

impl Host {
    /// Keeps a new grant with its first tokens, and logs its sign-in.
    pub(crate) fn issue_grant(&self, grant: &Grant, tokens: &Tokens) -> Result<(), OAuthError> {
        self.store
            .issue(grant, tokens)
            .map_err(OAuthError::internal)?;
        let signed_in = format!("signed in {}", grant.client_id);
        let logged = self.record(
            &grant.group,
            &grant.member,
            Some(&grant.client_id),
            &signed_in,
        );
        logged.map_err(OAuthError::internal)?;
        tracing::info!(
            group = grant.group,
            member = grant.member,
            client = grant.client_id,
            source = grant.source,
            "signed in"
        );
        Ok(())
    }
}

/// Ends the grant an access or refresh token is of, as RFC 7009 lets a
/// client revoke either. Answers the same whether there was one.
pub async fn revoke(State(host): State<Arc<Host>>, form: OAuthForm) -> Response {
    let form = match form_of(form) {
        Ok(form) => form,
        Err(e) => return failed(&host, e),
    };
    if let Some(token) = form.get("token") {
        let revoked = host.store.revoke_token(&jose::sha256(token));
        tracing::info!(?revoked, "revoked");
    }
    answer(&host, StatusCode::OK, json!({}))
}
