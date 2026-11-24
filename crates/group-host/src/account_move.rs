//! Moving an account here from the console: the host signs in to the
//! account at its own server, for the person moving it, and does each step
//! of `import.rs` and `import_space.rs` itself.
//!
//! The person, signed in to the console, signs in to the account too
//! (`/console/move/login`), approving at its server what a move needs and
//! no more: a `createAccount` token for this host, its identity, and
//! reading its own repos and files in spaces. The host keeps those tokens
//! for them, one account at a time, until the move is finished or
//! forgotten, and then gives them back. Its calls to the account's server
//! are DPoP-bound, as its sign-ins are, and refresh the token when it
//! lapses, as it may while the server's code is in the mail.
//!
//! - `getAccountMove`: the account signed in for the caller to move, if
//!   any, and whether its move is started.
//! - `startAccountMove`: its token for this host, then the account and
//!   its spaces copied (`importAccount`, `importAccountSpace`), and its
//!   server asked to email the code that lets its identity move.
//! - `finishAccountMove`: with the code, its server signs the operation,
//!   and the move finishes (`completeImport`).
//! - `cancelAccountMove`: the tokens given back and forgotten, and an
//!   unfinished copy discarded.

use crate::Host;
use crate::api::{ANSWER_AT_MOST, REPO_AT_MOST, too_much};
use crate::console_login::{Pending, sign_in};
use crate::group_api::signed_in;
use crate::index_api::capped;
use crate::migration::creds;
use crate::rbac::INTERNAL;
use crate::store::Store;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::{Curve, Keypair, did, jose};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use reqwest::Method;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// How long the host keeps an account's tokens for a move not finished.
const KEPT_FOR: i64 = 24 * 60 * 60_000;
/// How long before its access token lapses the host refreshes it.
const EARLY: i64 = 30_000;

/// An account signed in for someone to move here: where it lives, and its
/// tokens there.
#[derive(Debug, Clone)]
pub struct Move {
    pub did: String,
    pub pds: String,
    issuer: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    key: Vec<u8>,
    access: String,
    refresh: Option<String>,
    expires_at: i64,
}

const MOVE_OF: &str = "SELECT did, pds, issuer, token_endpoint, revocation_endpoint, key, access, refresh, expires_at FROM account_move WHERE mover = ?1 AND started > ?2";
const KEEP_MOVE: &str = "INSERT INTO account_move (mover, did, pds, issuer, token_endpoint, revocation_endpoint, key, access, refresh, expires_at, started) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT (mover) DO UPDATE SET did = ?2, pds = ?3, issuer = ?4, token_endpoint = ?5, revocation_endpoint = ?6, key = ?7, access = ?8, refresh = ?9, expires_at = ?10, started = ?11";
const SWEEP_MOVES: &str = "DELETE FROM account_move WHERE started <= ?1 RETURNING did, pds, issuer, token_endpoint, revocation_endpoint, key, access, refresh, expires_at";
const RENEW_MOVE: &str =
    "UPDATE account_move SET access = ?2, refresh = ?3, expires_at = ?4 WHERE mover = ?1";

/// A kept move, from its row.
fn row(r: &rusqlite::Row) -> rusqlite::Result<Move> {
    Ok(Move {
        did: r.get(0)?,
        pds: r.get(1)?,
        issuer: r.get(2)?,
        token_endpoint: r.get(3)?,
        revocation_endpoint: r.get(4)?,
        key: r.get(5)?,
        access: r.get(6)?,
        refresh: r.get(7)?,
        expires_at: r.get(8)?,
    })
}

impl Store {
    /// The account `mover` signed in to move, while it is kept.
    pub fn move_of(&self, mover: &str, now: i64) -> Result<Option<Move>> {
        let conn = self.conn();
        let found = conn.query_row(MOVE_OF, params![mover, now - KEPT_FOR], row);
        Ok(found.optional()?)
    }

    /// Forgets every move kept since before `cutoff`: each, to give back.
    pub fn sweep_moves(&self, cutoff: i64) -> Result<Vec<Move>> {
        let conn = self.conn();
        let mut q = conn.prepare(SWEEP_MOVES)?;
        let rows = q.query_map([cutoff], row);
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Keeps `m` for `mover`, in place of any account they signed in
    /// to move before.
    pub fn keep_move(&self, mover: &str, m: &Move, now: i64) -> Result<()> {
        let values = params![
            mover,
            m.did,
            m.pds,
            m.issuer,
            m.token_endpoint,
            m.revocation_endpoint,
            m.key,
            m.access,
            m.refresh,
            m.expires_at,
            now
        ];
        self.conn().execute(KEEP_MOVE, values)?;
        Ok(())
    }

    /// Forgets the account `mover` signed in to move.
    pub fn forget_move(&self, mover: &str) -> Result<()> {
        let sql = "DELETE FROM account_move WHERE mover = ?1";
        self.conn().execute(sql, [mover])?;
        Ok(())
    }
}

/// `text` percent-encoded but for what a URI never needs to escape.
fn escaped(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The account's server, failing or saying no.
fn upstream(why: impl std::fmt::Display) -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_GATEWAY,
        "UpstreamFailure",
        format!("the account's server: {why}"),
    )
}

fn none_here() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "NotFound",
        "no account is signed in for you to move: sign in to it first",
    )
}

/// Whether a refusal's `WWW-Authenticate` challenge names `error`.
fn challenged(res: &reqwest::Response, error: &str) -> bool {
    let header = res.headers().get("www-authenticate");
    let header = header.and_then(|v| v.to_str().ok()).unwrap_or_default();
    header.contains(&format!("error=\"{error}\""))
}

impl Host {
    /// What a move asks of the account, beyond `atproto`: a token for
    /// this host's `createAccount`, its identity, and reading its own
    /// repos and files in spaces, read_self reaching nobody else's.
    pub(crate) fn move_scope(&self) -> String {
        let aud = escaped(&format!("{}#atgrp", self.config.service_did));
        format!(
            "rpc:com.atproto.server.createAccount?aud={aud} identity:* space:*?authority=*&action=read_self"
        )
    }

    /// Keeps the tokens a move's sign-in brought, for whoever is moving it.
    pub(crate) async fn keep_move(&self, p: &Pending, tokens: &Value) -> Result<()> {
        let lasts = tokens["expires_in"].as_i64().unwrap_or(300) * 1000;
        let now = now_millis();
        let mover = p.mover.as_deref().expect("a move's sign-in");
        if let Some(before) = self.store.move_of(mover, now).ok().flatten() {
            self.give_back(&before).await;
        }
        let m = Move {
            did: p.did.clone(),
            pds: p.pds.clone(),
            issuer: p.issuer.clone(),
            token_endpoint: p.token_endpoint.clone(),
            revocation_endpoint: p.revocation_endpoint.clone(),
            key: p.key.clone(),
            access: tokens["access_token"].as_str().unwrap_or_default().into(),
            refresh: tokens["refresh_token"].as_str().map(String::from),
            expires_at: now + lasts,
        };
        self.store.keep_move(mover, &m, now)
    }

    /// Forgets the moves kept longer than a host keeps one, by `now`, and
    /// gives their tokens back: how many.
    pub async fn sweep_moves(&self, now: i64) -> Result<usize> {
        let lapsed = self.store.sweep_moves(now - KEPT_FOR)?;
        for m in &lapsed {
            self.give_back(m).await;
        }
        Ok(lapsed.len())
    }

    /// Gives `m`'s tokens back to the account's server, as far as it hears.
    async fn give_back(&self, m: &Move) {
        let key = Keypair::from_bytes(Curve::P256, &m.key).expect("a key the host made");
        let ends: Vec<&String> = m.revocation_endpoint.iter().collect();
        let tokens = [Some(&m.access), m.refresh.as_ref()];
        let tokens: Vec<&String> = tokens.into_iter().flatten().collect();
        for url in ends {
            for token in &tokens {
                let form = [("token", token.as_str())];
                let revoked = self.console_post(url, &m.issuer, &key, &form).await;
                tracing::debug!(?revoked, did = m.did, "gave back a moving account's token");
            }
        }
    }

    /// A fresh access token for `m`, for `mover`, by its refresh token.
    async fn renew(&self, mover: &str, m: &mut Move) -> Result<(), XrpcError> {
        let key = Keypair::from_bytes(Curve::P256, &m.key).expect("a key the host made");
        let Some(refresh) = m.refresh.clone() else {
            return Err(XrpcError::new(
                StatusCode::BAD_REQUEST,
                "SignInAgain",
                "the account's sign-in has lapsed: sign in to it again",
            ));
        };
        let form = [("grant_type", "refresh_token"), ("refresh_token", &refresh)];
        let tokens = self
            .console_post(&m.token_endpoint, &m.issuer, &key, &form)
            .await;
        let tokens = tokens.map_err(upstream)?;
        let lasts = tokens["expires_in"].as_i64().unwrap_or(300) * 1000;
        m.access = tokens["access_token"].as_str().unwrap_or_default().into();
        m.refresh = tokens["refresh_token"]
            .as_str()
            .map(String::from)
            .or(m.refresh.take());
        m.expires_at = now_millis() + lasts;
        let params = params![mover, m.access, m.refresh, m.expires_at];
        let kept = self.store.conn().execute(RENEW_MOVE, params);
        kept.map_err(XrpcError::internal)?;
        Ok(())
    }

    /// `nsid` of the account's own server, as the account, for `mover`:
    /// a DPoP-bound call, once more with the nonce the server asks for and
    /// once more with a fresh token if it took the one it had as lapsed.
    async fn as_moving(
        &self,
        mover: &str,
        m: &mut Move,
        nsid: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<reqwest::Response, XrpcError> {
        let url = format!("{}/xrpc/{nsid}", m.pds);
        let (pinned, at) = self.pinned(&url).await.map_err(upstream)?;
        if m.expires_at - EARLY <= now_millis() {
            self.renew(mover, m).await?;
        }
        let method = if body.is_some() {
            Method::POST
        } else {
            Method::GET
        };
        let key = Keypair::from_bytes(Curve::P256, &m.key).expect("a key the host made");
        // Each asked for once: a nonce, and a renewed token.
        let (mut nonce, mut nonced, mut renewed) = (None::<String>, false, false);
        loop {
            let mut claims = json!({
                "jti": jose::random(12),
                "htm": method.as_str(),
                "htu": url,
                "iat": now_millis() / 1000,
                "ath": jose::sha256(&m.access),
            });
            if let Some(n) = &nonce {
                claims["nonce"] = json!(n);
            }
            let header = json!({"typ": "dpop+jwt", "jwk": jose::jwk(&key.public())});
            let proof = jose::sign(&key, &header, &claims);
            let mut req = pinned.request(method.clone(), at.clone()).query(query);
            req = req.header("authorization", format!("DPoP {}", m.access));
            req = req.header("dpop", proof);
            // A procedure that takes no body is sent none.
            if let Some(body) = body.filter(|b| !b.is_null()) {
                req = req.json(body);
            }
            let res = req.send().await.map_err(upstream)?;
            let asked = res
                .headers()
                .get("dpop-nonce")
                .and_then(|v| v.to_str().ok());
            let asked = asked.map(String::from);
            if res.status() == StatusCode::UNAUTHORIZED
                && challenged(&res, "use_dpop_nonce")
                && !nonced
            {
                (nonce, nonced) = (asked, true);
                continue;
            }
            if res.status() == StatusCode::UNAUTHORIZED
                && challenged(&res, "invalid_token")
                && !renewed
            {
                renewed = true;
                self.renew(mover, m).await?;
                continue;
            }
            return Ok(res);
        }
    }

    /// The JSON `nsid` of the account's server answers, or why not.
    async fn moving_json(
        &self,
        mover: &str,
        m: &mut Move,
        nsid: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value, XrpcError> {
        let res = self.as_moving(mover, m, nsid, query, body).await?;
        let status = res.status();
        let read = capped(res, ANSWER_AT_MOST).await.unwrap_or_default();
        let said: Value = serde_json::from_slice(&read).unwrap_or_default();
        if !status.is_success() {
            let why = said["message"].as_str().or(said["error"].as_str());
            return Err(upstream(format!(
                "{nsid}: {status} {}",
                why.unwrap_or_default()
            )));
        }
        Ok(said)
    }

    /// The bytes `nsid` of the account's server answers, and their type;
    /// none where it holds no repo of the account's.
    async fn moving_bytes(
        &self,
        mover: &str,
        m: &mut Move,
        nsid: &str,
        query: &[(&str, &str)],
    ) -> Result<Option<(Vec<u8>, String)>, XrpcError> {
        let res = self.as_moving(mover, m, nsid, query, None).await?;
        let kind = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        let kind = kind.unwrap_or("application/octet-stream").to_string();
        let status = res.status();
        let read = capped(res, REPO_AT_MOST).await;
        let bytes = read.ok_or(upstream(too_much(REPO_AT_MOST)))?;
        let said: Value = serde_json::from_slice(&bytes).unwrap_or_default();
        if status.is_success() {
            return Ok(Some((bytes.to_vec(), kind)));
        }
        let none = said["error"] == "RepoNotFound";
        let why = upstream(format!(
            "{nsid}: {status} {}",
            said["message"].as_str().unwrap_or_default()
        ));
        none.then_some(None).ok_or(why)
    }

    /// The account's spaces, each brought here as its server serves them:
    /// a space it governs first, then its files there, then its repo,
    /// which holds them. How many spaces, records and files.
    async fn bring_spaces(
        &self,
        mover: &str,
        m: &mut Move,
    ) -> Result<(usize, usize, usize), XrpcError> {
        let did = m.did.clone();
        let mut spaces = Vec::new();
        // Only a server that says it holds spaces has any to bring: most do
        // not yet, and answer for a method they lack in no one way.
        let url = format!("{}/xrpc/com.atproto.server.describeServer", m.pds);
        let described = self
            .public_json(&url, "its server")
            .await
            .map_err(upstream)?;
        let holds = described["spacesAlpha"] == true;
        let mut cursor = holds.then_some(String::new());
        while let Some(after) = cursor.take() {
            let mut page = vec![("limit", "100")];
            page.extend((!after.is_empty()).then_some(("cursor", after.as_str())));
            let listed = self
                .moving_json(mover, m, "com.atproto.space.listSpaces", &page, None)
                .await?;
            let uris = listed["spaces"].as_array().into_iter().flatten();
            spaces.extend(uris.filter_map(|s| s["space"].as_str().map(String::from)));
            cursor = next(&listed);
        }
        let (mut records, mut files) = (0, 0);
        for space in &spaces {
            let at = [("space", space.as_str()), ("repo", did.as_str())];
            let governs = space.starts_with(&format!("at://{did}/"));
            if governs {
                let meta = self
                    .moving_bytes(mover, m, "com.atproto.space.exportSpaceMeta", &at[..1])
                    .await?;
                let (meta, _) = meta.ok_or(upstream(format!("{space} exported nothing")))?;
                self.import_space_meta(&did, &meta)?;
            }
            let mut cursor = Some(String::new());
            while let Some(after) = cursor.take() {
                let mut page = at.to_vec();
                page.extend((!after.is_empty()).then_some(("cursor", after.as_str())));
                let listed = self
                    .moving_json(mover, m, "com.atproto.space.listBlobs", &page, None)
                    .await?;
                for cid in listed["cids"].as_array().into_iter().flatten() {
                    let cid = cid.as_str().unwrap_or_default();
                    let at = [at[0], at[1], ("cid", cid)];
                    let blob = self
                        .moving_bytes(mover, m, "com.atproto.space.getBlob", &at)
                        .await?;
                    let (bytes, kind) =
                        blob.ok_or(upstream(format!("{space} has no file {cid}")))?;
                    self.upload_blob(&did, &bytes, &kind)?;
                    files += 1;
                }
                cursor = next(&listed);
            }
            if let Some((repo, _)) = self
                .moving_bytes(mover, m, "com.atproto.space.getRepo", &at)
                .await?
            {
                records += self.import_space_repo(&did, &repo)?.1;
            }
        }
        Ok((spaces.len(), records, files))
    }
}

/// The cursor a listing's next page starts after, if it has one.
fn next(listed: &Value) -> Option<String> {
    let cursor = listed["cursor"].as_str().unwrap_or_default();
    (!cursor.is_empty()).then_some(cursor.to_string())
}

/// Signs in to an account to move here, for the person signed in to the
/// console: its server asked for what a move needs.
pub async fn login(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, XrpcError> {
    let me = host.console_caller(&headers).map_err(XrpcError::internal)?;
    let Some(me) = me else {
        let why = "Signing in failed: sign in to the console first";
        return Ok((StatusCode::UNAUTHORIZED, why).into_response());
    };
    let scope = format!("atproto {}", host.move_scope());
    sign_in(&host, &q, &scope, Some(me)).await
}

/// The account `me` signed in to move, and their move of it.
fn moving(host: &Host, me: &str) -> Result<Move, XrpcError> {
    let found = host
        .store
        .move_of(me, now_millis())
        .map_err(XrpcError::internal)?;
    found.ok_or_else(none_here)
}

/// The account signed in for the caller to move, if any.
pub async fn get_account_move(State(host): State<Arc<Host>>, headers: HeaderMap) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.getAccountMove")).await?;
    let Some(m) = host
        .store
        .move_of(&me, now_millis())
        .map_err(XrpcError::internal)?
    else {
        return Ok(Json(json!({})));
    };
    let doc = host.identity.document(&m.did).await.ok();
    let handle = doc.as_ref().and_then(did::handle);
    let pending = host
        .store
        .pending_import(&m.did)
        .map_err(XrpcError::internal)?;
    let started = pending.is_some_and(|p| p.founder == me);
    let mut out = json!({"did": m.did, "pds": m.pds, "started": started});
    if let Some(handle) = handle {
        out["handle"] = json!(handle);
    }
    Ok(Json(out))
}

/// Copies the account the caller signed in to move, its spaces with it,
/// and has its server email the code that lets its identity move.
pub async fn start_account_move(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.startAccountMove")).await?;
    let mut m = moving(&host, &me)?;
    let aud = format!("{}#atgrp", host.config.service_did);
    let asked = [
        ("aud", aud.as_str()),
        ("lxm", "com.atproto.server.createAccount"),
    ];
    let auth = "com.atproto.server.getServiceAuth";
    let token = host.moving_json(&me, &mut m, auth, &asked, None).await?;
    let token = token["token"].as_str().unwrap_or_default();
    let mut copied = host.import_as(&me, token, body["address"].as_str()).await?;
    let (spaces, records, files) = host.bring_spaces(&me, &mut m).await?;
    let ask = "com.atproto.identity.requestPlcOperationSignature";
    host.moving_json(&me, &mut m, ask, &[], Some(&Value::Null))
        .await?;
    copied["spaces"] = json!({"spaces": spaces, "records": records, "blobs": files});
    Ok(Json(copied))
}

/// With the code the account's server emailed, has it sign its identity
/// over, and finishes the move; the account's tokens are given back.
pub async fn finish_account_move(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.finishAccountMove")).await?;
    let mut m = moving(&host, &me)?;
    let code = body["code"].as_str().map(str::trim).unwrap_or_default();
    let pending = host
        .store
        .pending_import(&m.did)
        .map_err(XrpcError::internal)?;
    if !pending.is_some_and(|p| p.founder == me) {
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "NotStarted",
            "start the move first",
        ));
    }
    let account = host.store.account(&m.did).map_err(XrpcError::internal)?;
    let mut asked = creds(&host, &account.expect("a move's account"));
    asked["token"] = json!(code);
    let sign = "com.atproto.identity.signPlcOperation";
    let signed = host
        .moving_json(&me, &mut m, sign, &[], Some(&asked))
        .await?;
    let did = m.did.clone();
    let done = host
        .complete_import_as(&me, &did, &signed["operation"])
        .await?;
    host.give_back(&m).await;
    host.store.forget_move(&me).map_err(XrpcError::internal)?;
    Ok(Json(done))
}

/// Forgets the account the caller signed in to move, its tokens given
/// back, and discards a copy of it not yet finished.
pub async fn cancel_account_move(State(host): State<Arc<Host>>, headers: HeaderMap) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.cancelAccountMove")).await?;
    let m = moving(&host, &me)?;
    let pending = host
        .store
        .pending_import(&m.did)
        .map_err(XrpcError::internal)?;
    if pending.is_some_and(|p| p.founder == me) {
        host.forget(&m.did);
    }
    host.give_back(&m).await;
    host.store.forget_move(&me).map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
