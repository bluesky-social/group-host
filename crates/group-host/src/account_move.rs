//! Moving an account here from the console: the host signs in to the
//! account at its own server, for the person moving it, and does each step
//! of `import.rs` and `import_space.rs` itself.
//!
//! The person, signed in to the console, signs in to the account too
//! (`/console/move/login`), approving at its server what a move needs and
//! no more: a `createAccount` token for this host, its identity, and
//! reading its own repos and files in spaces. The console's OAuth client
//! (`console_login::client`) keeps that session in the database,
//! by the account ([`Moves`]), and the host keeps which account each
//! person is moving, one at a time, until the move is finished or
//! forgotten, and then gives the session back. Its calls to the account's
//! server go through that client, DPoP-bound, refreshing the session when
//! it is about to lapse, and once more when the server takes it as lapsed,
//! as it may while the server's code is in the mail.
//!
//! - `getAccountMove`: the account signed in for the caller to move, if
//!   any, and whether its move is started.
//! - `startAccountMove`: its token for this host, then the account and
//!   its spaces copied (`importAccount`, `importAccountSpace`), and its
//!   server asked to email the code that lets its identity move.
//! - `finishAccountMove`: with the code, its server signs the operation,
//!   and the move finishes (`completeImport`).
//! - `cancelAccountMove`: the session given back and forgotten, and an
//!   unfinished copy discarded.

use crate::Host;
use crate::api::{ANSWER_AT_MOST, REPO_AT_MOST};
use crate::console_login::{come_back, done, sign_in, unstored};
use crate::group_api::signed_in;
use crate::migration::creds;
use crate::rbac::INTERNAL;
use crate::store::Store;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::did;
use atproto::space::SPACE_URI_PREFIX;
use atproto_client::oauth::{self as login, Session, SessionStore, StoreError};
use atproto_client::service_auth;
use atproto_client::xrpc::{self, Auth, Xrpc};
use axum::Json;
use axum::extract::{Query, RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// How long the host keeps an account's session for a move not finished.
const KEPT_FOR: i64 = 24 * 60 * 60_000;

/// The sessions of accounts being moved here, in the database, by the
/// account: the store of the console's client for moves.
pub(crate) struct Moves(pub Store);

impl SessionStore for Moves {
    fn get(&self, did: &str) -> Result<Option<Session>, StoreError> {
        self.0.oauth_session(did).map_err(unstored)
    }

    /// A session kept only while its account's move is: one whose move
    /// was finished, cancelled or swept meanwhile has ended.
    fn put(&self, session: &Session) -> Result<(), StoreError> {
        match self.0.keep_oauth_session(session).map_err(unstored)? {
            true => Ok(()),
            false => Err(StoreError::Ended),
        }
    }

    fn delete(&self, did: &str) -> Result<(), StoreError> {
        self.0.forget_oauth_session(did).map_err(unstored)
    }
}

const MOVE_OF: &str = "SELECT s.session FROM account_move m JOIN move_session s ON s.did = m.did WHERE m.mover = ?1 AND m.started > ?2";
const KEEP_MOVE: &str = "INSERT INTO account_move (mover, did, started) VALUES (?1, ?2, ?3) ON CONFLICT (mover) DO UPDATE SET did = ?2, started = ?3";
const KEEP_SESSION: &str = "INSERT INTO move_session (did, session) SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM account_move WHERE did = ?1) ON CONFLICT (did) DO UPDATE SET session = ?2";

impl Store {
    /// The session of the account `mover` signed in to move, while it is
    /// kept.
    pub fn move_of(&self, mover: &str, now: i64) -> Result<Option<Session>> {
        let conn = self.conn();
        let found = conn.query_row(MOVE_OF, params![mover, now - KEPT_FOR], |r| r.get(0));
        let found: Option<String> = found.optional()?;
        Ok(found.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    /// Forgets every move kept since before `cutoff`: each account's DID,
    /// to give its session back.
    pub fn sweep_moves(&self, cutoff: i64) -> Result<Vec<String>> {
        let conn = self.conn();
        let sql = "DELETE FROM account_move WHERE started <= ?1 RETURNING did";
        let mut q = conn.prepare(sql)?;
        let rows = q.query_map([cutoff], |r| r.get(0));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Keeps that `mover` is moving `did`, at `now`, in place of any account
    /// they signed in to move before, and of anyone else's move of it: the
    /// account they were moving before, if another, to give back.
    pub fn keep_move(&self, mover: &str, did: &str, now: i64) -> Result<Option<String>> {
        let conn = self.conn();
        let sql = "SELECT did FROM account_move WHERE mover = ?1";
        let before: Option<String> = conn.query_row(sql, [mover], |r| r.get(0)).optional()?;
        let others = "DELETE FROM account_move WHERE did = ?1 AND mover != ?2";
        conn.execute(others, [did, mover])?;
        conn.execute(KEEP_MOVE, params![mover, did, now])?;
        Ok(before.filter(|b| b != did))
    }

    /// Forgets the account `mover` signed in to move.
    pub fn forget_move(&self, mover: &str) -> Result<()> {
        let sql = "DELETE FROM account_move WHERE mover = ?1";
        self.conn().execute(sql, [mover])?;
        Ok(())
    }

    /// The session kept for the account `did`, being moved here.
    pub fn oauth_session(&self, did: &str) -> Result<Option<Session>> {
        let sql = "SELECT session FROM move_session WHERE did = ?1";
        let found = self.conn().query_row(sql, [did], |r| r.get(0));
        let found: Option<String> = found.optional()?;
        Ok(found.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    /// Keeps `session`, in place of any for its account, while the account
    /// is being moved: whether it is.
    pub fn keep_oauth_session(&self, session: &Session) -> Result<bool> {
        let kept = serde_json::to_string(session)?;
        let conn = self.conn();
        Ok(conn.execute(KEEP_SESSION, params![session.did, kept])? == 1)
    }

    /// Forgets the session kept for `did`.
    pub fn forget_oauth_session(&self, did: &str) -> Result<()> {
        let sql = "DELETE FROM move_session WHERE did = ?1";
        self.conn().execute(sql, [did])?;
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

fn sign_in_again() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "SignInAgain",
        "the account's sign-in has lapsed: sign in to it again",
    )
}

/// Whether the account's server refused a call as its token lapsed: a 401
/// that does not ask for a nonce, which the client has sent again with.
fn lapsed(e: &xrpc::Error) -> bool {
    e.xrpc()
        .is_some_and(|r| r.status == 401 && r.error != "use_dpop_nonce")
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

    /// Forgets the moves kept longer than a host keeps one, by `now`, and
    /// gives their sessions back: how many.
    pub async fn sweep_moves(&self, now: i64) -> Result<usize> {
        let lapsed = self.store.sweep_moves(now - KEPT_FOR)?;
        for did in &lapsed {
            self.give_back(did).await;
        }
        Ok(lapsed.len())
    }

    /// Gives `did`'s session back to its server, as far as it hears, and
    /// forgets it.
    async fn give_back(&self, did: &str) {
        let given = self.console.revoke(did).await;
        tracing::debug!(?given, did, "gave back a moving account's session");
    }

    /// `did`'s session, refreshed if it is about to lapse, or `now_anyway`.
    async fn moving_session(&self, did: &str, now_anyway: bool) -> Result<Session, XrpcError> {
        let session = match now_anyway {
            true => self.console.refresh(did).await.map(|r| r.session),
            false => self.console.session(did).await,
        };
        session.map_err(|e| match e {
            login::Error::NoRefresh(_) => sign_in_again(),
            e if e.refusal().is_some_and(|r| r.error == "invalid_grant") => sign_in_again(),
            // Finished or cancelled while it was refreshed.
            login::Error::Ended(_) => none_here(),
            e => upstream(e),
        })
    }

    /// The account `did`'s server, read at most `at_most` bytes of each
    /// answer, and what lets a call there in, its session refreshed first
    /// if `renew`, or if it is about to lapse.
    async fn moving_at(
        &self,
        did: &str,
        renew: bool,
        at_most: usize,
    ) -> Result<(Xrpc, Auth), XrpcError> {
        let session = self.moving_session(did, renew).await?;
        let auth = self.console.auth(&session).map_err(upstream)?;
        Ok((self.console.pds(&session).at_most(at_most), auth))
    }

    /// `call` of the account's server, as the account `did`, made once
    /// more, the session refreshed, if the server takes it as lapsed; of
    /// each answer, at most `at_most` bytes read. `nsid` names it in a
    /// failure.
    async fn as_moving<T, F>(
        &self,
        (did, nsid, at_most): (&str, &str, usize),
        call: impl Fn(Xrpc, Auth) -> F,
    ) -> Result<T, XrpcError>
    where
        F: Future<Output = Result<T, xrpc::Error>>,
    {
        let (pds, auth) = self.moving_at(did, false, at_most).await?;
        let said = match call(pds, auth).await {
            Err(e) if lapsed(&e) => {
                let (pds, auth) = self.moving_at(did, true, at_most).await?;
                call(pds, auth).await
            }
            said => said,
        };
        said.map_err(|e| upstream(format!("{nsid}: {e}")))
    }

    /// The JSON `nsid` of the account's server answers: a query with
    /// `query`, or with `body`, a procedure (one that takes none with a
    /// null body).
    async fn moving_json(
        &self,
        did: &str,
        nsid: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value, XrpcError> {
        let input = body.filter(|b| !b.is_null());
        let call = |pds: Xrpc, auth: Auth| async move {
            match body {
                None => pds.query(nsid, query, Some(&auth)).await,
                Some(_) => pds.procedure(nsid, input, Some(&auth)).await,
            }
        };
        self.as_moving((did, nsid, ANSWER_AT_MOST), call).await
    }

    /// The bytes `nsid` of the account's server answers, and their type;
    /// none where it holds no repo of the account's.
    async fn moving_bytes(
        &self,
        did: &str,
        nsid: &str,
        query: &[(&str, &str)],
    ) -> Result<Option<(Vec<u8>, String)>, XrpcError> {
        let call = |pds: Xrpc, auth: Auth| async move {
            let read = pds.query_raw(nsid, query, Some(&auth)).await;
            let errored = read.as_ref().err().and_then(xrpc::Error::xrpc);
            match errored.is_some_and(|r| r.error == "RepoNotFound") {
                true => Ok(None),
                false => read.map(Some),
            }
        };
        let read = self.as_moving((did, nsid, REPO_AT_MOST), call).await?;
        Ok(read.map(|(headers, bytes)| {
            let kind = headers.get("content-type").and_then(|v| v.to_str().ok());
            let kind = kind.unwrap_or("application/octet-stream").to_string();
            (bytes.to_vec(), kind)
        }))
    }

    /// The account's spaces, each brought here as its server serves them:
    /// a space it governs first, then its files there, then its repo,
    /// which holds them. How many spaces, records and files.
    async fn bring_spaces(&self, m: &Session) -> Result<(usize, usize, usize), XrpcError> {
        let did = m.did.as_str();
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
                .moving_json(did, "com.atproto.space.listSpaces", &page, None)
                .await?;
            let uris = listed["spaces"].as_array().into_iter().flatten();
            spaces.extend(uris.filter_map(|s| s["space"].as_str().map(String::from)));
            cursor = next(&listed);
        }
        let (mut records, mut files) = (0, 0);
        for space in &spaces {
            let at = [("space", space.as_str()), ("repo", did)];
            let governs = space.starts_with(&format!("{SPACE_URI_PREFIX}{did}/"));
            if governs {
                let meta = self
                    .moving_bytes(did, "com.atproto.space.exportSpaceMeta", &at[..1])
                    .await?;
                let (meta, _) = meta.ok_or(upstream(format!("{space} exported nothing")))?;
                self.import_space_meta(did, &meta)?;
            }
            let mut cursor = Some(String::new());
            while let Some(after) = cursor.take() {
                let mut page = at.to_vec();
                page.extend((!after.is_empty()).then_some(("cursor", after.as_str())));
                let listed = self
                    .moving_json(did, "com.atproto.space.listBlobs", &page, None)
                    .await?;
                for cid in listed["cids"].as_array().into_iter().flatten() {
                    let cid = cid.as_str().unwrap_or_default();
                    let at = [at[0], at[1], ("cid", cid)];
                    let blob = self
                        .moving_bytes(did, "com.atproto.space.getBlob", &at)
                        .await?;
                    let (bytes, kind) =
                        blob.ok_or(upstream(format!("{space} has no file {cid}")))?;
                    self.upload_blob(did, &bytes, &kind)?;
                    files += 1;
                }
                cursor = next(&listed);
            }
            if let Some((repo, _)) = self
                .moving_bytes(did, "com.atproto.space.getRepo", &at)
                .await?
            {
                records += self.import_space_repo(did, &repo)?.1;
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
    Ok(sign_in(&host, &q, Some(me)).await)
}

/// The browser comes back from approving a move's sign-in: the account's
/// session kept for the one moving it, who stays signed in as themselves,
/// in place of any account they signed in to move before, which is given
/// back.
pub async fn callback(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, XrpcError> {
    let query = query.unwrap_or_default();
    let (session, started) = match come_back(&host, &headers, &query, true).await? {
        Ok(back) => back,
        Err(refused) => return Ok(refused),
    };
    // A move's sign-in carries who moves it. Its session is kept while the
    // move is, in place of any kept for the account before, which is given
    // back, as is the account they were moving before, if another.
    let mover = started.mover.unwrap_or_default();
    let before = host.store.keep_move(&mover, &session.did, now_millis());
    if let Some(before) = before.map_err(XrpcError::internal)? {
        host.give_back(&before).await;
    }
    let kept = host.console.keep(&session).await;
    kept.map_err(XrpcError::internal)?;
    tracing::info!(did = session.did, mover, "signed in to an account to move");
    let done = [(header::SET_COOKIE, done(&host))];
    Ok((done, Redirect::to(&started.back)).into_response())
}

/// The session of the account `me` signed in to move.
fn moving(host: &Host, me: &str) -> Result<Session, XrpcError> {
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
    let doc = host.identity.document_now(&m.did).await.ok();
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
    let m = moving(&host, &me)?;
    let aud = format!("{}#atgrp", host.config.service_did);
    let lxm = "com.atproto.server.createAccount";
    let ask = |pds: Xrpc, auth: Auth| {
        let aud = &aud;
        async move { service_auth::token(&pds, &auth, aud, Some(lxm), None).await }
    };
    let nsid = "com.atproto.server.getServiceAuth";
    let token = host.as_moving((&m.did, nsid, ANSWER_AT_MOST), ask).await?;
    let mut copied = host
        .import_as(&me, &token, body["address"].as_str())
        .await?;
    let (spaces, records, files) = host.bring_spaces(&m).await?;
    let ask = "com.atproto.identity.requestPlcOperationSignature";
    host.moving_json(&m.did, ask, &[], Some(&Value::Null))
        .await?;
    copied["spaces"] = json!({"spaces": spaces, "records": records, "blobs": files});
    Ok(Json(copied))
}

/// With the code the account's server emailed, has it sign its identity
/// over, and finishes the move; the account's session is given back.
pub async fn finish_account_move(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.finishAccountMove")).await?;
    let m = moving(&host, &me)?;
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
    let signed = host.moving_json(&m.did, sign, &[], Some(&asked)).await?;
    let done = host
        .complete_import_as(&me, &m.did, &signed["operation"])
        .await?;
    host.store.forget_move(&me).map_err(XrpcError::internal)?;
    host.give_back(&m.did).await;
    Ok(Json(done))
}

/// Forgets the account the caller signed in to move, its session given
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
    host.store.forget_move(&me).map_err(XrpcError::internal)?;
    host.give_back(&m.did).await;
    Ok(Json(json!({})))
}
