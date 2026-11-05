//! Assistants: an AI app, as an MCP client, acting as a member in one of
//! their groups, with what the member's roles there allow and nothing
//! more.
//!
//! The host is the assistant's MCP server, at `/mcp`, and the OAuth server
//! it signs in at, as MCP's authorization spec has it: OAuth 2.1 with PKCE,
//! Bearer tokens for that one resource, the server found from the
//! resource's metadata (RFC 9728) and named by its own (RFC 8414, at the
//! issuer `<PUBLIC_URL>/mcp`). An assistant names itself by a client ID
//! metadata document, its client ID the URL of one, or registers itself
//! (RFC 7591). This is not the host's atproto OAuth server, where apps
//! sign in *as* a group, and their tokens are not each other's.
//!
//! Where the member says yes is the console's approval page, signed in as
//! themselves: the assistant's request is one more the page reads with
//! `getAuthorization`, and the member picks the group. Each tool call is
//! then the member's own, in that group, as the console's are: the host
//! asks itself, as them. Put out of the group, they and their assistants
//! can do nothing there, and its grants end.

use crate::Host;
use crate::api::{Params, param};
use crate::group_api::{group_of, signed_in};
use crate::rbac::NS;
use crate::store::Store;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::jose;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, Json};
use group_host_mcp::Tool;
use rusqlite::{OptionalExtension, Row, params};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tower::ServiceExt;

/// What a request an assistant made is named by, on the approval page.
pub const PREFIX: &str = "urn:group-host:assistant:";
/// How long an assistant's request waits to be approved, and its code to
/// be exchanged.
const REQUEST_FOR: i64 = 10 * 60_000;
/// How long its access token lasts, and its refresh token unused.
const ACCESS_FOR: i64 = 60 * 60_000;
const REFRESH_FOR: i64 = 30 * 24 * 60 * 60_000;
/// How long a registered client that never connected is kept.
const UNUSED_FOR: i64 = 24 * 60 * 60_000;

/// The tools, made once.
static TOOLS: LazyLock<Vec<Tool>> = LazyLock::new(group_host_mcp::tools);

/// An assistant's request to act as a member: who it is, where to send
/// the browser back to, and once approved, the group and member.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uri: String,
    pub state: Option<String>,
    pub code_challenge: String,
    pub expires_at: i64,
    pub group: Option<String>,
    pub member: Option<String>,
}

/// An assistant connected: the member it acts as, in the group, and when.
#[derive(Debug, Clone, PartialEq)]
pub struct Connected {
    pub id: String,
    pub group: String,
    pub member: String,
    pub client_id: String,
    pub client_name: String,
    pub created_at: i64,
    pub used_at: i64,
}

fn asked(r: &Row) -> rusqlite::Result<Asked> {
    Ok(Asked {
        id: r.get(0)?,
        client_id: r.get(1)?,
        client_name: r.get(2)?,
        redirect_uri: r.get(3)?,
        state: r.get(4)?,
        code_challenge: r.get(5)?,
        expires_at: r.get(6)?,
        group: r.get(7)?,
        member: r.get(8)?,
    })
}

fn connected(r: &Row) -> rusqlite::Result<Connected> {
    Ok(Connected {
        id: r.get(0)?,
        group: r.get(1)?,
        member: r.get(2)?,
        client_id: r.get(3)?,
        client_name: r.get(4)?,
        created_at: r.get(5)?,
        used_at: r.get(6)?,
    })
}

const ASKED: &str = "SELECT id, client_id, client_name, redirect_uri, state, code_challenge, expires_at, grp, member FROM assistant_request";
const CONNECTED: &str =
    "SELECT id, grp, member, client_id, client_name, created_at, used_at FROM assistant_grant";

impl Store {
    /// Keeps a client an assistant registered: its name, and where it
    /// may be sent back to.
    pub fn register_assistant(
        &self,
        id: &str,
        name: &str,
        redirects: &[String],
        now: i64,
    ) -> Result<()> {
        // Registering is anyone's: one never connected goes after a day.
        // One a member connected stays, to connect again; its connections
        // go once their refresh tokens lapse.
        let stale = "DELETE FROM assistant_client WHERE created_at < ?1 AND connected = 0";
        self.conn().execute(stale, [now - UNUSED_FOR])?;
        let lapsed = "DELETE FROM assistant_grant WHERE expires_at <= ?1";
        self.conn().execute(lapsed, [now])?;
        let sql = "INSERT INTO assistant_client (id, name, redirect_uris, created_at) VALUES (?1, ?2, ?3, ?4)";
        let redirects = serde_json::to_string(redirects)?;
        self.conn()
            .execute(sql, params![id, name, redirects, now])?;
        Ok(())
    }

    /// A registered client's name and redirect URIs.
    pub fn registered_assistant(&self, id: &str) -> Result<Option<(String, Vec<String>)>> {
        let sql = "SELECT name, redirect_uris FROM assistant_client WHERE id = ?1";
        let found = self.conn().query_row(sql, [id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        });
        let found = found.optional()?;
        Ok(found.map(|(name, uris)| (name, serde_json::from_str(&uris).unwrap_or_default())))
    }

    /// Keeps an assistant's request, until it is answered or lapses.
    pub fn ask_for_assistant(&self, a: &Asked) -> Result<()> {
        let sql = "INSERT INTO assistant_request (id, client_id, client_name, redirect_uri, state, code_challenge, expires_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
        let row = params![
            a.id,
            a.client_id,
            a.client_name,
            a.redirect_uri,
            a.state,
            a.code_challenge,
            a.expires_at
        ];
        self.conn().execute(sql, row)?;
        Ok(())
    }

    /// An assistant's request, unanswered and unlapsed at `now`.
    pub fn assistant_request(&self, id: &str, now: i64) -> Result<Option<Asked>> {
        let sql = format!("{ASKED} WHERE id = ?1 AND expires_at > ?2 AND grp IS NULL");
        let found = self.conn().query_row(&sql, params![id, now], asked);
        Ok(found.optional()?)
    }

    /// Approves an assistant's request for `member` in `group`, its code
    /// kept by its hash: the request, if it was there to approve.
    pub fn approve_assistant(
        &self,
        id: &str,
        group: &str,
        member: &str,
        code: &str,
        now: i64,
    ) -> Result<Option<Asked>> {
        let sql = "UPDATE assistant_request SET grp = ?2, member = ?3, code_hash = ?4
                   WHERE id = ?1 AND grp IS NULL AND expires_at > ?5
                   RETURNING id, client_id, client_name, redirect_uri, state, code_challenge, expires_at, grp, member";
        let row = params![id, group, member, code, now];
        Ok(self.conn().query_row(sql, row, asked).optional()?)
    }

    /// An assistant's request, gone: whether it was there.
    pub fn drop_assistant_request(&self, id: &str) -> Result<bool> {
        let sql = "DELETE FROM assistant_request WHERE id = ?1";
        Ok(self.conn().execute(sql, [id])? == 1)
    }

    /// The approved request whose code hashes to `code`, once: it is gone
    /// as it is taken.
    pub fn redeem_assistant_code(&self, code: &str, now: i64) -> Result<Option<Asked>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let sql = format!("{ASKED} WHERE code_hash = ?1 AND expires_at > ?2 AND grp IS NOT NULL");
        let found = tx.query_row(&sql, params![code, now], asked).optional()?;
        tx.execute("DELETE FROM assistant_request WHERE code_hash = ?1", [code])?;
        tx.commit()?;
        Ok(found)
    }

    /// Keeps an assistant connected, its tokens by their hashes.
    pub fn connect_assistant(
        &self,
        c: &Connected,
        access: &str,
        refresh: &str,
        now: i64,
    ) -> Result<()> {
        let sql = "INSERT INTO assistant_grant (id, grp, member, client_id, client_name, access_hash, access_expires, refresh_hash, expires_at, created_at, used_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)";
        let row = params![
            c.id,
            c.group,
            c.member,
            c.client_id,
            c.client_name,
            access,
            now + ACCESS_FOR,
            refresh,
            now + REFRESH_FOR,
            now
        ];
        self.conn().execute(sql, row)?;
        let connected = "UPDATE assistant_client SET connected = 1 WHERE id = ?1";
        self.conn().execute(connected, [&c.client_id])?;
        Ok(())
    }

    /// The connection whose access token hashes to `access`, unlapsed,
    /// marked as used `now`.
    pub fn assistant_by_access(&self, access: &str, now: i64) -> Result<Option<Connected>> {
        let sql = format!("{CONNECTED} WHERE access_hash = ?1 AND access_expires > ?2");
        let found = self
            .conn()
            .query_row(&sql, params![access, now], connected)
            .optional()?;
        if let Some(c) = &found {
            let sql = "UPDATE assistant_grant SET used_at = ?2 WHERE id = ?1";
            self.conn().execute(sql, params![c.id, now])?;
        }
        Ok(found)
    }

    /// `client`'s connection whose refresh token hashes to `refresh`, its
    /// tokens changed to `access` and `next`: none if there is none, or it
    /// lapsed.
    pub fn refresh_assistant(
        &self,
        client: &str,
        refresh: &str,
        access: &str,
        next: &str,
        now: i64,
    ) -> Result<Option<Connected>> {
        let sql = "UPDATE assistant_grant SET access_hash = ?3, access_expires = ?4, refresh_hash = ?5, expires_at = ?6
                   WHERE refresh_hash = ?1 AND client_id = ?2 AND expires_at > ?7";
        let row = params![
            refresh,
            client,
            access,
            now + ACCESS_FOR,
            next,
            now + REFRESH_FOR,
            now
        ];
        if self.conn().execute(sql, row)? == 0 {
            return Ok(None);
        }
        let sql = format!("{CONNECTED} WHERE refresh_hash = ?1");
        Ok(self.conn().query_row(&sql, [next], connected).optional()?)
    }

    /// The assistants `member` connected to `group`, the most lately used
    /// first.
    pub fn assistants_of(&self, group: &str, member: &str, now: i64) -> Result<Vec<Connected>> {
        let sql = format!(
            "{CONNECTED} WHERE grp = ?1 AND member = ?2 AND expires_at > ?3 ORDER BY used_at DESC, id"
        );
        let conn = self.conn();
        let mut q = conn.prepare(&sql)?;
        let rows = q.query_map(params![group, member, now], connected)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// `member`'s connections of `client` in `group`, ended: how many.
    pub fn disconnect_assistant(&self, group: &str, member: &str, client: &str) -> Result<usize> {
        let sql = "DELETE FROM assistant_grant WHERE grp = ?1 AND member = ?2 AND client_id = ?3";
        Ok(self.conn().execute(sql, params![group, member, client])?)
    }
}

/// An OAuth error, as its endpoints answer one.
fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    let body = json!({"error": error, "error_description": description});
    (status, [(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

fn bad(error: &str, description: &str) -> Response {
    oauth_error(StatusCode::BAD_REQUEST, error, description)
}

impl Host {
    /// The MCP server's address, and the issuer of its tokens.
    fn mcp_url(&self) -> String {
        format!("{}/mcp", self.config.public_url)
    }

    /// Where an assistant finds out how to sign in.
    fn resource_metadata_url(&self) -> String {
        format!(
            "{}/.well-known/oauth-protected-resource/mcp",
            self.config.public_url
        )
    }

    /// A client's name, and the redirect URIs it may use: by its metadata
    /// document if its ID is the URL of one, or as it registered.
    async fn assistant_client(&self, client_id: &str) -> Option<(String, Vec<String>)> {
        // Over https, or http for a host on http, as on a dev network.
        let on_http = self.config.public_url.starts_with("http://");
        if client_id.starts_with("https://") || (on_http && client_id.starts_with("http://")) {
            let doc = self
                .public_json(client_id, "its client metadata")
                .await
                .ok()?;
            if doc["client_id"] != client_id {
                return None;
            }
            let name = doc["client_name"].as_str().unwrap_or(client_id).to_string();
            let uris = doc["redirect_uris"].as_array()?;
            let uris: Vec<String> = uris
                .iter()
                .filter_map(|u| u.as_str().map(String::from))
                .collect();
            // Held to what a registered client is: one URI it may not be
            // sent back to, and it is no client.
            return redirects_ok(&uris).then_some((name, uris));
        }
        self.store.registered_assistant(client_id).ok().flatten()
    }
}

/// The MCP server's resource metadata (RFC 9728).
pub async fn resource_metadata(State(host): State<Arc<Host>>) -> Json<Value> {
    Json(json!({
        "resource": host.mcp_url(),
        "authorization_servers": [host.mcp_url()],
        "bearer_methods_supported": ["header"],
        "resource_name": "Your groups",
    }))
}

/// Its authorization server's metadata (RFC 8414).
pub async fn server_metadata(State(host): State<Arc<Host>>) -> Json<Value> {
    let at = host.mcp_url();
    Json(json!({
        "issuer": at,
        "authorization_endpoint": format!("{at}/authorize"),
        "token_endpoint": format!("{at}/token"),
        "registration_endpoint": format!("{at}/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
    }))
}

/// Whether an assistant may be sent back to `uri`: over https, to a
/// loopback address over http, or to an app's own scheme; never with a
/// fragment (RFC 6749 §3.1.2).
fn may_return_to(uri: &str) -> bool {
    let Some(url) = reqwest::Url::parse(uri)
        .ok()
        .filter(|u| u.fragment().is_none())
    else {
        return false;
    };
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => true,
        "http" => loopback,
        "javascript" | "data" | "file" | "vbscript" => false,
        _ => true,
    }
}

/// The most redirect URIs a client names, and the longest one.
const REDIRECTS_MAX: usize = 10;
const REDIRECT_MAX: usize = 2048;

/// Whether `uris` are a client's to be sent back to: a few, none long,
/// each one it may be.
fn redirects_ok(uris: &[String]) -> bool {
    let few = (1..=REDIRECTS_MAX).contains(&uris.len());
    few && uris
        .iter()
        .all(|u| u.len() <= REDIRECT_MAX && may_return_to(u))
}

/// Registers an assistant (RFC 7591): a public client, by its name and
/// where it may be sent back to.
pub async fn register(
    State(host): State<Arc<Host>>,
    Json(body): Json<Value>,
) -> Result<Response, XrpcError> {
    let uris: Vec<String> = body["redirect_uris"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|u| u.as_str().map(String::from))
        .collect();
    if !redirects_ok(&uris) {
        let said = "redirect_uris are https, http to a loopback address, or an app's own scheme";
        return Ok(bad("invalid_redirect_uri", said));
    }
    let name = body["client_name"].as_str().unwrap_or("An assistant");
    let name: String = name.chars().take(100).collect();
    let id = jose::random(16);
    let kept = host
        .store
        .register_assistant(&id, &name, &uris, now_millis());
    kept.map_err(XrpcError::internal)?;
    let out = json!({
        "client_id": id,
        "client_name": name,
        "redirect_uris": uris,
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
    });
    Ok((StatusCode::CREATED, Json(out)).into_response())
}

/// `uri` with `query` added.
fn with_query(uri: &str, query: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse(uri).expect("a redirect URI checked as a URL");
    url.query_pairs_mut().extend_pairs(query);
    url.to_string()
}

/// An assistant's authorization request: checked, kept, and the browser
/// sent to the console to say yes to it.
pub async fn authorize(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
) -> Result<Response, XrpcError> {
    let refused = |why: &'static str| Ok((StatusCode::BAD_REQUEST, why).into_response());
    let (Some(client_id), Some(redirect_uri)) = (p.get("client_id"), p.get("redirect_uri")) else {
        return refused("client_id and redirect_uri are required");
    };
    let Some((name, uris)) = host.assistant_client(client_id).await else {
        return refused("no client by that client_id");
    };
    if !uris.contains(redirect_uri) {
        return refused("redirect_uri is not the client's");
    }
    let state = p.get("state").cloned();
    let iss = host.mcp_url();
    // Once the client is known, it hears why by its redirect URI.
    let back = |error: &str| {
        let mut query = vec![("error", error), ("iss", iss.as_str())];
        query.extend(state.as_deref().map(|s| ("state", s)));
        Ok(Redirect::to(&with_query(redirect_uri, &query)).into_response())
    };
    let challenge = p
        .get("code_challenge")
        .map(String::as_str)
        .unwrap_or_default();
    let s256 = p.get("code_challenge_method").map(String::as_str) == Some("S256");
    let resource = p.get("resource").is_none_or(|r| *r == iss);
    if p.get("response_type").map(String::as_str) != Some("code") {
        return back("unsupported_response_type");
    }
    if !s256 || !(43..=128).contains(&challenge.len()) || !resource {
        return back("invalid_request");
    }
    let request = Asked {
        id: format!("{PREFIX}{}", jose::random(24)),
        client_id: client_id.clone(),
        client_name: name,
        redirect_uri: redirect_uri.clone(),
        state: state.clone(),
        code_challenge: challenge.into(),
        expires_at: now_millis() + REQUEST_FOR,
        group: None,
        member: None,
    };
    let kept = host.store.ask_for_assistant(&request);
    kept.map_err(XrpcError::internal)?;
    let console = &host.config.console_url;
    let approve = with_query(
        &format!("{console}/authorize"),
        &[("request_uri", &request.id)],
    );
    Ok(Redirect::to(&approve).into_response())
}

/// The pending request `uri` names, for the console: the client, and the
/// groups the member could connect it to.
pub(crate) fn assistant_view(host: &Host, uri: &str, me: &str) -> Result<Value, XrpcError> {
    let request = host.store.assistant_request(uri, now_millis());
    let request = request.map_err(XrpcError::internal)?.ok_or_else(expired)?;
    let mut groups = Vec::new();
    // It asks for no scope: it does what the member's roles allow.
    for group in host.groups_of(me)? {
        groups.push(host.offered(&group, me, &[])?.0);
    }
    Ok(json!({
        "client": {"clientId": request.client_id, "name": request.client_name},
        "requested": [],
        "labels": [],
        "groups": groups,
        "assistant": true,
    }))
}

pub(crate) fn expired() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "ExpiredRequest",
        "this sign-in request has expired; go back to the app and try again",
    )
}

/// Approves an assistant's request for `me` in `group`: where to send the
/// browser back to, with its code.
pub(crate) fn approve(host: &Host, uri: &str, me: &str, group: &str) -> Result<String, XrpcError> {
    group_of(host, group)?;
    host.active(group)?;
    let roles = host.roles(group, me).map_err(XrpcError::internal)?;
    if roles.is_empty() {
        let said = "you hold no role in this group";
        return Err(XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", said));
    }
    let code = jose::random(32);
    let hash = jose::sha256(&code);
    let approved = host
        .store
        .approve_assistant(uri, group, me, &hash, now_millis());
    let request = approved.map_err(XrpcError::internal)?.ok_or_else(expired)?;
    tracing::info!(
        member = me,
        client = request.client_id,
        group,
        "an assistant connected"
    );
    let iss = host.mcp_url();
    let mut query = vec![("code", code.as_str()), ("iss", iss.as_str())];
    if let Some(s) = &request.state {
        query.push(("state", s));
    }
    Ok(with_query(&request.redirect_uri, &query))
}

/// Declines an assistant's request: where to send the browser back to.
pub(crate) fn decline(host: &Host, uri: &str) -> Result<String, XrpcError> {
    let request = host.store.assistant_request(uri, now_millis());
    let request = request.map_err(XrpcError::internal)?.ok_or_else(expired)?;
    host.store
        .drop_assistant_request(uri)
        .map_err(XrpcError::internal)?;
    let iss = host.mcp_url();
    let mut query = vec![("error", "access_denied"), ("iss", iss.as_str())];
    if let Some(s) = &request.state {
        query.push(("state", s));
    }
    Ok(with_query(&request.redirect_uri, &query))
}

/// Two new tokens, and their hashes.
fn tokens() -> ((String, String), (String, String)) {
    let (access, refresh) = (jose::random(32), jose::random(32));
    let hashes = (jose::sha256(&access), jose::sha256(&refresh));
    ((access, refresh), hashes)
}

fn issued(access: &str, refresh: &str) -> Response {
    let body = json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": ACCESS_FOR / 1000,
        "refresh_token": refresh,
    });
    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

/// The token endpoint: a code exchanged, its PKCE verifier checked, or a
/// refresh token for new ones.
pub async fn token(
    State(host): State<Arc<Host>>,
    Form(f): Form<HashMap<String, String>>,
) -> Result<Response, XrpcError> {
    let field = |k: &str| f.get(k).map(String::as_str).unwrap_or_default();
    let now = now_millis();
    let ((access, refresh), (access_hash, refresh_hash)) = tokens();
    match field("grant_type") {
        "authorization_code" => {
            let redeemed = host
                .store
                .redeem_assistant_code(&jose::sha256(field("code")), now);
            let Some(request) = redeemed.map_err(XrpcError::internal)? else {
                return Ok(bad("invalid_grant", "an unknown or used code"));
            };
            let verifier = field("code_verifier");
            let proved = (43..=128).contains(&verifier.len())
                && jose::sha256(verifier) == request.code_challenge;
            if request.client_id != field("client_id")
                || request.redirect_uri != field("redirect_uri")
                || !proved
            {
                let said = "the code is not this client's, or not for this verifier";
                return Ok(bad("invalid_grant", said));
            }
            let connection = Connected {
                id: jose::random(16),
                group: request.group.unwrap_or_default(),
                member: request.member.unwrap_or_default(),
                client_id: request.client_id,
                client_name: request.client_name,
                created_at: now,
                used_at: now,
            };
            let kept = host
                .store
                .connect_assistant(&connection, &access_hash, &refresh_hash, now);
            kept.map_err(XrpcError::internal)?;
            Ok(issued(&access, &refresh))
        }
        "refresh_token" => {
            let old = jose::sha256(field("refresh_token"));
            let client = field("client_id");
            let rotated =
                host.store
                    .refresh_assistant(client, &old, &access_hash, &refresh_hash, now);
            match rotated.map_err(XrpcError::internal)? {
                Some(_) => Ok(issued(&access, &refresh)),
                None => Ok(bad(
                    "invalid_grant",
                    "an unknown, used or lapsed refresh token",
                )),
            }
        }
        _ => Ok(bad(
            "unsupported_grant_type",
            "authorization_code or refresh_token",
        )),
    }
}

/// The challenge an assistant without a current token is answered with.
fn challenge(host: &Host, error: Option<&str>) -> Response {
    let meta = host.resource_metadata_url();
    let said = match error {
        Some(e) => format!(r#"Bearer error="{e}", resource_metadata="{meta}""#),
        None => format!(r#"Bearer resource_metadata="{meta}""#),
    };
    let headers = [(header::WWW_AUTHENTICATE, said)];
    (
        StatusCode::UNAUTHORIZED,
        headers,
        "an assistant's token is required",
    )
        .into_response()
}

/// A tool's arguments as a query's parameters: each value as text, a
/// list's each in turn.
fn query_of(args: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (k, v) in args.as_object().into_iter().flatten() {
        let items = match v {
            Value::Array(items) => items.clone(),
            one => vec![one.clone()],
        };
        for item in items {
            let text = match item {
                Value::String(s) => s,
                other => other.to_string(),
            };
            out.push((k.clone(), text));
        }
    }
    out
}

impl Host {
    /// Calls `tool` with `args` as `who`, in their group: as the console
    /// does, the host asking itself as them. A tool that writes an invite
    /// makes the write their own session for the group would.
    async fn call_as(
        self: &Arc<Self>,
        who: &Connected,
        tool: Tool,
        mut args: Value,
    ) -> Result<Value, String> {
        if tool.nsid.starts_with("com.atproto.space.") {
            let made = self.invite_as(who, &tool.name, &args).await;
            return made.map_err(|e| format!("{}: {}", e.error, e.message));
        }
        args["group"] = json!(who.group);
        let cookie = self.session_cookie(&who.member, now_millis());
        let cookie = format!("{}={cookie}", crate::console_login::COOKIE);
        let path = format!("/xrpc/{}", tool.nsid);
        let request = match tool.procedure {
            true => Request::post(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(args.to_string())),
            false => {
                let mut url = reqwest::Url::parse("http://host/").expect("a URL");
                url.set_path(&path);
                url.query_pairs_mut().extend_pairs(query_of(&args));
                let at = format!("{}?{}", url.path(), url.query().unwrap_or_default());
                Request::get(at).body(Body::empty())
            }
        };
        let mut request = request.expect("a request of a path and a body");
        let value = cookie.parse().expect("a cookie is a header's value");
        request.headers_mut().insert(header::COOKIE, value);
        let res = crate::router(self.clone()).oneshot(request).await;
        let res = res.expect("the router answers every request");
        let ok = res.status().is_success();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await;
        let body: Value = bytes
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if !ok {
            let error = body["error"].as_str().unwrap_or("Error");
            let message = body["message"].as_str().unwrap_or_default();
            return Err(format!("{error}: {message}"));
        }
        Ok(body)
    }
}

/// The MCP server: each message an assistant sends, answered as the
/// member it acts as, in their group.
pub async fn mcp(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, XrpcError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|a| a.strip_prefix("Bearer "));
    let Some(token) = token else {
        return Ok(challenge(&host, None));
    };
    let found = host
        .store
        .assistant_by_access(&jose::sha256(token), now_millis());
    let Some(who) = found.map_err(XrpcError::internal)? else {
        return Ok(challenge(&host, Some("invalid_token")));
    };
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        let said = Json(group_host_mcp::not_json());
        return Ok((StatusCode::BAD_REQUEST, said).into_response());
    };
    let call = |tool: Tool, args: Value| async move { host.call_as(&who, tool, args).await };
    Ok(match group_host_mcp::answer(&message, &TOOLS, call).await {
        Some(answer) => Json(answer).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    })
}

/// The assistants the caller connected to a group.
pub async fn list_connected_apps(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Json<Value>, XrpcError> {
    let me = signed_in(&host, &headers, &format!("{NS}.listConnectedApps")).await?;
    let group = param(&p, "group")?;
    group_of(&host, group)?;
    let connected = host.store.assistants_of(group, &me, now_millis());
    let apps: Vec<Value> = connected
        .map_err(XrpcError::internal)?
        .iter()
        .map(|c| {
            json!({
                "clientId": c.client_id,
                "name": c.client_name,
                "connectedAt": iso(c.created_at),
                "usedAt": iso(c.used_at),
            })
        })
        .collect();
    Ok(Json(json!({ "apps": apps })))
}

/// Disconnects an assistant the caller connected to a group.
pub async fn revoke_connected_app(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, XrpcError> {
    let me = signed_in(&host, &headers, &format!("{NS}.revokeConnectedApp")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    let client = body["clientId"].as_str();
    let client = client.expect("its lexicon requires it");
    group_of(&host, group)?;
    let ended = host.store.disconnect_assistant(group, &me, client);
    ended.map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
