//! Signing in to the console, as oneself.
//!
//! The host is a confidential OAuth client of each person's own PDS, under
//! a client id of its own, its key derived from the host's secret, through
//! atproto-client's OAuth client ([`login::Client`]): the request pushed,
//! the browser sent to approve it, the code redeemed, and the tokens taken
//! only once the account's own authorization server, followed again from
//! its DID, is the one that issued them. It asks for `atproto` alone: the
//! console's methods act as the group, never as the person, so the host
//! needs only to know who they are. Once their PDS says so, it revokes
//! what it was given, and keeps nothing of theirs. A request waiting for
//! the browser to come back is kept in the database by its state
//! ([`Requests`]), so any of the host's processes answers it.
//!
//! The browser holds only a cookie the host signed, naming the person,
//! HttpOnly and SameSite=Lax; where the console is on https, named
//! `__Host-` first, so the browser keeps it as the console's alone,
//! Secure and for its whole path, and nothing at a name beside it can set
//! one in its place. A request that says it comes from a page elsewhere
//! (an `Origin` other than the console's) is as nobody's, whatever cookie
//! it carries, and signs nobody in or out. A sign-in is answered only in
//! the browser that started it, by a cookie naming a secret its request
//! carries, so a link to someone else's approval cannot sign a person in
//! as them. Signing out refuses every cookie signed before.
//!
//! A browser can be signed in to several accounts at once: signing in to
//! another keeps the one before in a second cookie, of such signed
//! cookies, the one used last first, and switching (`/console/switch`)
//! swaps one of those for the one in use, with no sign-in. Signing out
//! goes on as the one used last, if any is left.
//!
//! Every server a person's handle leads to (their PDS, its authorization
//! server) is reached only at a public address, as a client's metadata is.
//!
//! The console serves `/console/*` from the host, beside its pages.

use crate::Host;
use crate::config::Config;
use crate::oauth_dpop::hmac;
use crate::observe;
use crate::secrets::Secrets;
use crate::store::Store;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::oauth::{Callback, ClientKeys, WebClient, client_jwk};
use atproto::{Curve, Keypair, jose};
use atproto_client::guard::Guarded;
use atproto_client::identity::Resolver;
use atproto_client::oauth::{self as login, Authorize, Session, StateStore, StoreError};
use axum::extract::{Query, RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use axum::{Form, Json};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// The session cookie, and the cookie naming a sign-in under way, as
/// named before `__Host-` (`Host::cookie_name`).
pub const COOKIE: &str = "group_host_session";
const STARTED: &str = "group_host_login";
/// The cookie holding the other accounts a browser is signed in to: their
/// session cookies, `~` between, the one used last first.
const OTHERS: &str = "group_host_accounts";
/// How many other accounts a browser stays signed in to; signing in to one
/// more lets go of the one used least lately.
const MOST: usize = 4;
/// How long a sign-in lasts, and how long approving one may take.
const LASTS: i64 = 30 * 86_400_000;
const TAKES: i64 = login::PENDING_FOR as i64 * 1000;
/// The console's key, as its client metadata names it.
const KID: &str = "console";

/// What a sign-in carries to the browser's return, beside what the OAuth
/// client keeps: the page to go back to, the secret the browser that
/// started it holds in its cookie, and, for an account being moved here,
/// who is moving it.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Started {
    pub back: String,
    browser: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mover: Option<String>,
}

/// The requests waiting for the browser to come back, in the database:
/// both of the console's OAuth clients keep theirs here.
pub(crate) struct Requests(pub Store);

/// A store's failure, as the OAuth client takes it.
pub(crate) fn unstored(e: anyhow::Error) -> StoreError {
    StoreError(format!("{e:#}"))
}

impl StateStore for Requests {
    fn put(&self, state: &str, pending: &login::Pending) -> Result<(), StoreError> {
        self.0.put_request(state, pending).map_err(unstored)
    }

    fn take(&self, state: &str) -> Result<Option<login::Pending>, StoreError> {
        self.0.take_request(state).map_err(unstored)
    }
}

impl Store {
    /// Keeps a request waiting for the browser by its state; those lapsed
    /// by the time it was pushed are forgotten.
    pub fn put_request(&self, state: &str, p: &login::Pending) -> anyhow::Result<()> {
        let started = i64::try_from(p.started)?;
        let conn = self.conn();
        let lapsed = started - login::PENDING_FOR as i64;
        conn.execute("DELETE FROM console_request WHERE started < ?1", [lapsed])?;
        let values = params![state, serde_json::to_string(p)?, started];
        conn.execute(PUT_REQUEST, values)?;
        Ok(())
    }

    /// The request `state` names, forgotten as it is read.
    pub fn take_request(&self, state: &str) -> anyhow::Result<Option<login::Pending>> {
        let sql = "DELETE FROM console_request WHERE state = ?1 RETURNING pending";
        let conn = self.conn();
        let found = conn.query_row(sql, [state], |r| r.get(0)).optional();
        let found: Option<String> = found?;
        Ok(found.map(|p| serde_json::from_str(&p)).transpose()?)
    }

    /// The request `state` names, left as it is.
    pub fn request(&self, state: &str) -> anyhow::Result<Option<login::Pending>> {
        let sql = "SELECT pending FROM console_request WHERE state = ?1";
        let conn = self.conn();
        let found = conn.query_row(sql, [state], |r| r.get(0)).optional();
        let found: Option<String> = found?;
        Ok(found.map(|p| serde_json::from_str(&p)).transpose()?)
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

const PUT_REQUEST: &str =
    "INSERT INTO console_request (state, pending, started) VALUES (?1, ?2, ?3)";
const SIGN_OUT: &str =
    "INSERT INTO console_logout (did, at) VALUES (?1, ?2) ON CONFLICT (did) DO UPDATE SET at = ?2";

/// The key the host signs its client assertions with, as the console.
fn console_key(secrets: &Secrets) -> Keypair {
    let secret = hmac(
        &secrets.rotation.to_bytes(),
        b"group-host console client key",
    );
    Keypair::from_bytes(Curve::P256, &secret).expect("an HMAC is a P-256 secret")
}

fn client_id(config: &Config) -> String {
    format!("{}/console/client-metadata.json", config.console_url)
}

/// The console's two OAuth clients, under its one client id and key, each
/// call they make through [`observe::Calls`]: one signing people in, who
/// keeps no session past its callback, and one signing in to accounts
/// being moved here, keeping their sessions in the database until the move
/// is done ([`crate::account_move::Moves`]). They are two, each with a
/// callback of its own, as a sign-in ends any session its client keeps
/// for the same account: someone signing in to the console as an account
/// would otherwise end its move.
pub(crate) fn clients(
    config: &Config,
    secrets: &Secrets,
    store: &Store,
    guarded: &Guarded,
    identity: &Resolver,
) -> (login::Client, login::Client) {
    let id = client_id(config);
    let requests: Arc<dyn StateStore> = Arc::new(Requests(store.clone()));
    let client = || {
        let credentials = login::Credentials::private_key_jwt(console_key(secrets), KID);
        let credentials = credentials.expect("a P-256 key");
        login::Client::new(&id, credentials, guarded.clone(), identity.clone())
            .states(requests.clone())
            .sending(Arc::new(observe::Calls))
    };
    let moves = Arc::new(crate::account_move::Moves(store.clone()));
    (client(), client().sessions(moves))
}

fn failed(status: StatusCode, why: impl Into<String>) -> Response {
    let why = why.into();
    tracing::info!(why, "console sign-in refused");
    (status, format!("Signing in failed: {why}")).into_response()
}

/// Why a sign-in could not start or finish, as the browser is told.
fn refused(e: &login::Error) -> Response {
    use login::Error;
    let (status, why) = match e {
        Error::UnknownState => (StatusCode::BAD_REQUEST, TOOK_TOO_LONG.to_string()),
        Error::Identity(_) | Error::Handle { .. } => (
            StatusCode::BAD_REQUEST,
            format!("no account we can find: {e}"),
        ),
        Error::Refused(_) | Error::Invalid(_) => (StatusCode::BAD_REQUEST, e.to_string()),
        Error::OAuth(_) | Error::WrongAccount { .. } | Error::Issuer(_) => {
            (StatusCode::FORBIDDEN, e.to_string())
        }
        Error::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        _ => (StatusCode::BAD_GATEWAY, e.to_string()),
    };
    failed(status, why)
}

const TOOK_TOO_LONG: &str = "this browser did not start that sign-in, or it took too long";

/// A cookie, as the host sets one: for the whole site, HttpOnly, Lax, and
/// Secure, named `__Host-`, where the console is on https.
pub(crate) fn cookie(host: &Host, name: &str, value: &str, max_age: i64) -> String {
    let secure = if host.secure() { "; Secure" } else { "" };
    let name = host.cookie_name(name);
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}

/// The `Set-Cookie` that ends a sign-in under way.
pub(crate) fn done(host: &Host) -> String {
    cookie(host, STARTED, "", 0)
}

/// Refuses what only the console's own pages may ask: signing in to
/// another account, or out.
fn elsewhere() -> Response {
    let why = "only the console's own pages sign in and out";
    failed(StatusCode::FORBIDDEN, why)
}

/// The `Set-Cookie` keeping `others`, as many as a browser keeps: none
/// clears it.
fn others_cookie(host: &Host, others: &[(String, String)]) -> String {
    let kept: Vec<&str> = others.iter().take(MOST).map(|(_, v)| v.as_str()).collect();
    let max_age = if kept.is_empty() { 0 } else { LASTS / 1000 };
    cookie(host, OTHERS, &kept.join("~"), max_age)
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
    /// Whether the console is on https, where its cookies are Secure.
    fn secure(&self) -> bool {
        self.config.console_url.starts_with("https://")
    }

    /// A cookie's name, as the host sets it: `__Host-` first where the
    /// console is on https.
    pub fn cookie_name(&self, name: &str) -> String {
        match self.secure() {
            true => format!("__Host-{name}"),
            false => name.to_string(),
        }
    }

    /// The value of the cookie `name` a request carries.
    fn cookie_of<'a>(&self, headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
        let name = self.cookie_name(name);
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|kv| kv.trim().split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v)
    }

    /// Whether a request says it comes from a page somewhere other than
    /// the console: by an `Origin` other than the console's own.
    fn asked_elsewhere(&self, headers: &HeaderMap) -> bool {
        let console = reqwest::Url::parse(&self.config.console_url).expect("CONSOLE_URL is a URL");
        let ours = console.origin().ascii_serialization();
        headers
            .get(header::ORIGIN)
            .is_some_and(|o| o.as_bytes() != ours.as_bytes())
    }

    pub(crate) fn console_callback(&self) -> String {
        format!("{}/console/callback", self.config.console_url)
    }

    pub(crate) fn move_callback(&self) -> String {
        format!("{}/console/move/callback", self.config.console_url)
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
        if self.asked_elsewhere(headers) {
            return Ok(None);
        }
        match self.cookie_of(headers, COOKIE) {
            Some(value) => self.session_of(value),
            None => Ok(None),
        }
    }

    /// The other accounts a request's cookies hold, with their session
    /// cookies, the one used last first: only those that still hold, each
    /// once, and never `but`.
    pub fn console_others(
        &self,
        headers: &HeaderMap,
        but: Option<&str>,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let mut found: Vec<(String, String)> = Vec::new();
        for value in self
            .cookie_of(headers, OTHERS)
            .unwrap_or_default()
            .split('~')
        {
            let Some(did) = self.session_of(value)? else {
                continue;
            };
            if Some(did.as_str()) != but && found.iter().all(|(d, _)| *d != did) {
                found.push((did, value.to_string()));
            }
        }
        Ok(found)
    }

    /// Who the session cookie `value` names, while it lasts and they have
    /// not signed out since it was signed.
    fn session_of(&self, value: &str) -> anyhow::Result<Option<String>> {
        let Some((claims, sig)) = value.split_once('.') else {
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
}

/// The console's client metadata, at its client id: as atproto's profile
/// has a confidential web client's, which only a console on https at a
/// public name can have.
pub async fn client_metadata(State(host): State<Arc<Host>>) -> Result<Json<Value>, XrpcError> {
    let key = [client_jwk(&console_key(&host.secrets).public(), KID)];
    let (id, callbacks) = (
        client_id(&host.config),
        [host.console_callback(), host.move_callback()],
    );
    let callbacks: Vec<&str> = callbacks.iter().map(String::as_str).collect();
    let scope = format!("atproto {}", host.move_scope());
    let client = WebClient {
        client_id: &id,
        client_name: Some("Group host"),
        client_uri: Some(&host.config.console_url),
        redirect_uris: &callbacks,
        scope: &scope,
        keys: ClientKeys::Jwks(&key),
    };
    let meta = client.metadata().map_err(|e| {
        let why = format!("CONSOLE_URL cannot be an OAuth client's: {e}");
        XrpcError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalServerError",
            why,
        )
    })?;
    Ok(Json(meta))
}

/// Starts signing `handle` in: pushes the request to their PDS's
/// authorization server, and sends the browser there to approve it. With
/// `create`, and no handle, signs someone up instead, at `SIGNUP_PDS`.
pub async fn login(
    State(host): State<Arc<Host>>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, XrpcError> {
    Ok(sign_in(&host, &q, None).await)
}

/// Starts a sign-in to `q`'s `handle`, for `mover` where it is an account
/// they are moving here: asking what a move needs, by the client that keeps
/// moves' sessions.
pub(crate) async fn sign_in(
    host: &Host,
    q: &HashMap<String, String>,
    mover: Option<String>,
) -> Response {
    let identifier = q.get("handle").map(|h| h.trim()).filter(|h| !h.is_empty());
    // A sign-up names no one yet: the account it makes is checked as it
    // comes back. Never for an account being moved, which is someone's.
    let create = identifier.is_none() && q.contains_key("create") && mover.is_none();
    let input = match (identifier, create) {
        (Some(who), _) => who,
        (None, true) => host.config.signup_pds.as_str(),
        (None, false) => return failed(StatusCode::BAD_REQUEST, "say who you are"),
    };
    let (client, redirect_uri, scope) = match mover {
        Some(_) => (
            &host.moving,
            host.move_callback(),
            format!("atproto {}", host.move_scope()),
        ),
        None => (&host.console, host.console_callback(), "atproto".into()),
    };
    let started = Started {
        back: back_to(q.get("back")),
        browser: jose::random(16),
        mover,
    };
    let carried = serde_json::to_string(&started).expect("JSON");
    let ask = Authorize {
        redirect_uri: &redirect_uri,
        scope: &scope,
        prompt: create.then_some("create"),
        app_state: Some(&carried),
    };
    let to = match client.authorize(input, &ask).await {
        Ok(to) => to,
        Err(e) => return refused(&e),
    };
    let held = cookie(host, STARTED, &started.browser, TAKES / 1000);
    ([(header::SET_COOKIE, held)], Redirect::to(&to)).into_response()
}

/// The session the browser's return, `query`, grants by `client`, at its
/// `callback`, and what its sign-in carried: only in the browser that
/// started it, at the callback it was started for, and for an account
/// being moved, while the one moving it is still the one signed in. Else
/// the answer refusing it. The request is answered once; one refused
/// before its code is redeemed is left to lapse.
pub(crate) async fn come_back(
    host: &Host,
    headers: &HeaderMap,
    query: &str,
    client: &login::Client,
    callback: &str,
) -> Result<Result<(Session, Started), Response>, XrpcError> {
    let state = Callback::parse(query).state.unwrap_or_default();
    let pending = host.store.request(&state).map_err(XrpcError::internal)?;
    let started = pending
        .filter(|p| p.redirect_uri == callback)
        .and_then(|p| serde_json::from_str::<Started>(&p.app_state?).ok())
        .filter(|s| host.cookie_of(headers, STARTED) == Some(s.browser.as_str()));
    let Some(started) = started else {
        return Ok(Err(failed(StatusCode::BAD_REQUEST, TOOK_TOO_LONG)));
    };
    let caller = host.console_caller(headers).map_err(XrpcError::internal)?;
    if started.mover.is_some() && caller != started.mover {
        let why = "sign in to the console again first";
        return Ok(Err(failed(StatusCode::FORBIDDEN, why)));
    }
    Ok(match client.callback(query).await {
        Ok((session, _)) => Ok((session, started)),
        Err(e) => Err(refused(&e)),
    })
}

/// The browser comes back from approving: the code is exchanged, the
/// person is who they said, and the host signs them in.
pub async fn callback(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, XrpcError> {
    let query = query.unwrap_or_default();
    let at = host.console_callback();
    let back = come_back(&host, &headers, &query, &host.console, &at).await?;
    let (session, started) = match back {
        Ok(back) => back,
        Err(refused) => return Ok(refused),
    };
    let did = session.did;
    // Knowing who they are is all the host wanted: it keeps nothing.
    let revoked = host.console.revoke(&did).await;
    tracing::debug!(?revoked, "gave back the console's tokens");
    let now = now_millis();
    let session = cookie(&host, COOKIE, &host.session_cookie(&did, now), LASTS / 1000);
    // The account in use before, if another, is kept beside it.
    let before = host.console_caller(&headers).map_err(XrpcError::internal)?;
    let mut others = host
        .console_others(&headers, Some(&did))
        .map_err(XrpcError::internal)?;
    if let (Some(was), Some(value)) = (before, host.cookie_of(&headers, COOKIE))
        && was != did
    {
        others.retain(|(d, _)| *d != was);
        others.insert(0, (was, value.to_string()));
    }
    tracing::info!(did, "signed in to the console");
    let mut res = Redirect::to(&started.back).into_response();
    let set = res.headers_mut();
    for c in [session, done(&host)] {
        set.append(header::SET_COOKIE, c.parse().expect("a cookie is a header"));
    }
    if !others.is_empty() || host.cookie_of(&headers, OTHERS).is_some() {
        let kept = others_cookie(&host, &others);
        set.append(
            header::SET_COOKIE,
            kept.parse().expect("a cookie is a header"),
        );
    }
    Ok(res)
}

/// Switches the browser to `did`, another account it is signed in to,
/// and back to the page `back`: its cookie becomes the one in use, and the
/// one in use is kept beside it, first.
pub async fn switch(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Response, XrpcError> {
    if host.asked_elsewhere(&headers) {
        return Ok(elsewhere());
    }
    let to = form.get("did").map(String::as_str).unwrap_or_default();
    let current = host.console_caller(&headers).map_err(XrpcError::internal)?;
    let mut others = host
        .console_others(&headers, current.as_deref())
        .map_err(XrpcError::internal)?;
    let Some(at) = others.iter().position(|(did, _)| did == to) else {
        let why = "this browser is not signed in to that account";
        return Ok(failed(StatusCode::BAD_REQUEST, why));
    };
    let (_, value) = others.remove(at);
    if let (Some(did), Some(was)) = (current, host.cookie_of(&headers, COOKIE)) {
        others.insert(0, (did, was.to_string()));
    }
    let headers = AppendHeaders([
        (
            header::SET_COOKIE,
            cookie(&host, COOKIE, &value, LASTS / 1000),
        ),
        (header::SET_COOKIE, others_cookie(&host, &others)),
    ]);
    Ok((headers, Redirect::to(&back_to(form.get("back")))).into_response())
}

/// Signs out: this cookie, and every one signed before now. The browser
/// goes on as the other account it used last, if it is signed in to one.
pub async fn logout(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
) -> Result<Response, XrpcError> {
    if host.asked_elsewhere(&headers) {
        return Ok(elsewhere());
    }
    let who = host.console_caller(&headers).map_err(XrpcError::internal)?;
    if let Some(did) = &who {
        host.store
            .sign_out(did, now_millis())
            .map_err(XrpcError::internal)?;
    }
    // Read once they are signed out, so none of their cookies is left.
    let mut others = host
        .console_others(&headers, who.as_deref())
        .map_err(XrpcError::internal)?;
    let next = match others.is_empty() {
        true => cookie(&host, COOKIE, "", 0),
        false => cookie(&host, COOKIE, &others.remove(0).1, LASTS / 1000),
    };
    let mut res = Redirect::to("/").into_response();
    let set = res.headers_mut();
    set.append(
        header::SET_COOKIE,
        next.parse().expect("a cookie is a header"),
    );
    if host.cookie_of(&headers, OTHERS).is_some() {
        let kept = others_cookie(&host, &others);
        set.append(
            header::SET_COOKIE,
            kept.parse().expect("a cookie is a header"),
        );
    }
    Ok(res)
}
