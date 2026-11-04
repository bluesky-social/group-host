//! The `com.atproto.space.*` methods a repo host answers for its accounts'
//! repos in spaces: reading them, for whoever holds a credential the
//! space's authority issued, each answer vouched for with a MAC under a
//! secret agreed for the request; and being told a credential is revoked.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::store::{Account, Record, SpaceHead};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::service_auth::{self, Claims};
use atproto::space::{
    COMMIT_MAC, Commit, LtHash, RECORD_MAC, SpaceRef, decapsulate, mac_header, parse_want_mac,
    record_mac, star,
};
use atproto::space_auth::{self, CLOCK_SKEW, CREDENTIAL_MAX_AGE, Expect, Kind, TokenError};
use atproto::{Cid, drisl};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::sync::Arc;

/// The method an authority revokes credentials with.
pub const NOTIFY_REVOKED: &str = "com.atproto.space.notifyCredentialRevoked";

const WANT_MAC: &str = "atproto-space-want-mac";
const MAC: HeaderName = HeaderName::from_static("atproto-space-mac");

fn unauthorized(code: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::UNAUTHORIZED, code, message)
}

fn refused(e: TokenError) -> XrpcError {
    unauthorized(e.code, e.message)
}

fn now() -> u64 {
    now_millis() as u64 / 1000
}

/// What a read in a space asks of: the account whose repo it is, and the
/// space, once the caller's credential lets them read it.
struct Asked {
    account: Account,
    space: String,
}

/// Checks the request's space credential (`Authorization:
/// Atproto-Space-Credential <jwt>`): issued by the space's authority, for
/// this space and this repo, unrevoked, and the request signed with the key
/// it is bound to.
async fn credential(
    host: &Host,
    headers: &HeaderMap,
    space: &str,
    repo: &str,
) -> Result<(), XrpcError> {
    let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let jwt = text("authorization")
        .and_then(|a| a.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Atproto-Space-Credential"))
        .map(|(_, jwt)| jwt.trim());
    let jwt = jwt.ok_or_else(|| unauthorized("MissingJwt", "missing space credential"))?;
    let (_, token) = space_auth::parse(Kind::Credential, jwt).map_err(refused)?;
    let issued_for = SpaceRef::parse(&token.sub)
        .map_err(|_| unauthorized("BadJwtSub", "space credential subject is not a space"))?;
    if token.iss != issued_for.authority {
        return Err(unauthorized(
            "BadJwtIss",
            "space credential issuer is not the space authority",
        ));
    }
    let expect = Expect {
        aud: None,
        sub: Some(space),
    };
    let check =
        |key: &atproto::PublicKey| space_auth::verify(Kind::Credential, jwt, key, &expect, now());
    let checked = host.identity.with_key(&token.iss, check).await;
    let checked = checked
        .ok_or_else(|| unauthorized("BadJwtIss", format!("no signing key for {}", token.iss)))?;
    checked.map_err(refused)?;
    if text("atproto-space-audience") != Some(repo) {
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "BadSpaceAudience",
            "the space audience is not the repo asked for",
        ));
    }
    let cnf = token.cnf.as_deref().unwrap_or_default();
    space_auth::verify_request(text, cnf).map_err(refused)?;
    let revoked = host.store.revoked(space, &token.jti, now() as i64);
    if revoked.map_err(XrpcError::internal)? {
        return Err(unauthorized(
            "CredentialRevoked",
            "space credential has been revoked",
        ));
    }
    Ok(())
}

/// The space and repo a read names, once its credential is checked.
async fn asked(host: &Host, p: &Params, headers: &HeaderMap) -> Result<Asked, XrpcError> {
    let space = param(p, "space")?;
    let space = SpaceRef::parse(space).map_err(|e| invalid(e.to_string()))?;
    let space = space.to_string();
    let repo = param(p, "repo")?;
    credential(host, headers, &space, repo).await?;
    let account = host.store.account(repo).map_err(XrpcError::internal)?;
    let account = account.filter(|a| a.did == repo).ok_or_else(|| {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "RepoNotFound",
            format!("no repo {repo} here"),
        )
    })?;
    Ok(Asked { account, space })
}

/// The secret agreed for the request, from the key share its
/// `Atproto-Space-Want-Mac` header for a MAC of `kind` sends.
fn secret(headers: &HeaderMap, account: &Account, kind: &str) -> Result<[u8; 32], XrpcError> {
    let header = headers.get(WANT_MAC).and_then(|v| v.to_str().ok());
    let share = parse_want_mac(header.unwrap_or_default(), kind);
    let agreed = share.and_then(|share| decapsulate(&account.kem, &share));
    agreed.map_err(|e| XrpcError::new(StatusCode::BAD_REQUEST, "BadSpaceMac", e.0))
}

/// An answer for one reader: not kept by a shared cache, and varying with
/// the credential and the key share.
fn private(mut res: Response, mac: Option<String>) -> Response {
    let headers = res.headers_mut();
    headers.insert(header::CACHE_CONTROL, "private".parse().unwrap());
    let vary = format!("authorization, {WANT_MAC}");
    headers.insert(header::VARY, vary.parse().unwrap());
    if let Some(mac) = mac {
        headers.insert(MAC, mac.parse().expect("a MAC header is ASCII"));
    }
    res
}

/// The repo's commit, from its state.
fn commit(asked: &Asked, head: &SpaceHead) -> Commit {
    let hash = LtHash::from_state(&head.state).expect("a state the host wrote");
    Commit {
        repo: asked.account.did.clone(),
        space: asked.space.clone(),
        rev: head.rev.clone(),
        data: hash.digest(),
    }
}

fn commit_json(c: &Commit) -> Value {
    json!({
        "ver": atproto::space::COMMIT_VERSION,
        "repo": c.repo,
        "space": c.space,
        "rev": c.rev,
        "data": atproto::Value::Bytes(c.data.to_vec()).to_json(),
    })
}

fn no_repo_in(space: &str) -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "RepoNotFound",
        format!("no repo in {space}"),
    )
}

fn head(host: &Host, asked: &Asked) -> Result<SpaceHead, XrpcError> {
    let head = host
        .store
        .space_head(&asked.account.did, &asked.space)
        .map_err(XrpcError::internal)?;
    head.ok_or_else(|| no_repo_in(&asked.space))
}

fn value(bytes: &[u8]) -> Value {
    drisl::decode(bytes)
        .expect("a record the host wrote")
        .to_json()
}

/// A record's URI in a space: the space's, then the repo and the path.
fn uri(asked: &Asked, collection: &str, rkey: &str) -> String {
    format!("{}/{}/{collection}/{rkey}", asked.space, asked.account.did)
}

type Answer = Result<Response, XrpcError>;

pub async fn get_record(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asked = asked(&host, &p, &headers).await?;
    let secret = secret(&headers, &asked.account, RECORD_MAC)?;
    let (collection, rkey) = (param(&p, "collection")?, param(&p, "rkey")?);
    let found = host
        .store
        .space_record(&asked.account.did, &asked.space, collection, rkey)
        .map_err(XrpcError::internal)?;
    let uri = uri(&asked, collection, rkey);
    let record = found.ok_or_else(|| {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "RecordNotFound",
            format!("no record {uri}"),
        )
    })?;
    let mac = mac_header(RECORD_MAC, &record_mac(&secret, &uri, &record.cid));
    let body = json!({"uri": uri, "cid": record.cid.to_string(), "value": value(&record.value)});
    Ok(private(Json(body).into_response(), Some(mac)))
}

pub async fn list_records(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asked = asked(&host, &p, &headers).await?;
    let limit = page(&p, 50, 1000)?;
    let flag = |name: &str| p.get(name).is_some_and(|v| v == "true");
    let prefix = format!("{}/{}/", asked.space, asked.account.did);
    let cursor = match p.get("cursor") {
        None => None,
        Some(c) => Some(
            c.strip_prefix(&prefix)
                .ok_or_else(|| invalid("the cursor is not a record of this repo"))?,
        ),
    };
    let collection = p.get("collection").map(String::as_str);
    let records = host
        .store
        .space_list(
            &asked.account.did,
            &asked.space,
            collection,
            cursor,
            limit,
            flag("reverse"),
        )
        .map_err(XrpcError::internal)?;
    let cursor = records
        .last()
        .filter(|_| records.len() == limit as usize)
        .map(|r: &Record| uri(&asked, &r.collection, &r.rkey));
    let records: Vec<Value> = records
        .iter()
        .map(|r| {
            let mut entry =
                json!({"collection": r.collection, "rkey": r.rkey, "cid": r.cid.to_string()});
            if !flag("excludeValues") {
                entry["value"] = value(&r.value);
            }
            entry
        })
        .collect();
    let body = json!({"records": records, "cursor": cursor});
    Ok(private(Json(body).into_response(), None))
}

/// The whole repo, as a STAR archive, its commit vouched for.
pub async fn get_repo(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asked = asked(&host, &p, &headers).await?;
    let secret = secret(&headers, &asked.account, COMMIT_MAC)?;
    let head = head(&host, &asked)?;
    let records = host
        .store
        .space_records(&asked.account.did, &asked.space)
        .map_err(XrpcError::internal)?;
    let commit = commit(&asked, &head);
    let mac = mac_header(COMMIT_MAC, &commit.mac(&secret));
    let res = (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        star(&commit, &records),
    );
    Ok(private(res.into_response(), Some(mac)))
}

pub async fn get_latest_commit(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asked = asked(&host, &p, &headers).await?;
    let secret = secret(&headers, &asked.account, COMMIT_MAC)?;
    let commit = commit(&asked, &head(&host, &asked)?);
    let mac = mac_header(COMMIT_MAC, &commit.mac(&secret));
    let body = json!({"commit": commit_json(&commit)});
    Ok(private(Json(body).into_response(), Some(mac)))
}

/// The changes after `since`, a page at a time; the last page carries the
/// commit they lead to, vouched for, for the syncer to check its running
/// set hash against.
pub async fn list_repo_ops(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asked = asked(&host, &p, &headers).await?;
    let secret = secret(&headers, &asked.account, COMMIT_MAC)?;
    let limit = page(&p, 100, 1000)?;
    let after = match p.get("cursor") {
        Some(c) => c
            .split_once('/')
            .and_then(|(rev, idx)| Some((rev, idx.parse().ok()?)))
            .ok_or_else(|| {
                XrpcError::new(
                    StatusCode::BAD_REQUEST,
                    "MalformedCursor",
                    "malformed cursor",
                )
            })?,
        // Every op of a revision after `since`, or every op.
        None => (p.get("since").map_or("", String::as_str), i64::MAX),
    };
    let (ops, head) = host
        .store
        .space_ops(&asked.account.did, &asked.space, after, limit)
        .map_err(XrpcError::internal)?;
    // A full page is not the end: a cursor to go on from, and no commit.
    let full = ops.len() == limit as usize;
    let commit = head.filter(|_| !full).map(|h| commit(&asked, &h));
    let cursor = ops
        .last()
        .filter(|_| full)
        .map(|op| format!("{}/{}", op.rev, op.idx));
    let exclude = p.get("excludeValues").is_some_and(|v| v == "true");
    let ops: Vec<Value> = ops
        .iter()
        .map(|op| {
            let mut entry = json!({
                "rev": op.rev,
                "collection": op.collection,
                "rkey": op.rkey,
                "cid": op.cid.map(|c| c.to_string()),
                "prev": op.prev.map(|c| c.to_string()),
            });
            if let Some(v) = op.value.as_deref().filter(|_| !exclude) {
                entry["value"] = value(v);
            }
            entry
        })
        .collect();
    let mac = commit
        .as_ref()
        .map(|c| mac_header(COMMIT_MAC, &c.mac(&secret)));
    let body = json!({"ops": ops, "cursor": cursor, "commit": commit.as_ref().map(commit_json)});
    Ok(private(Json(body).into_response(), mac))
}

/// A blob a record of the repo in the space references, served as a
/// download, as `com.atproto.sync.getBlob` serves one.
pub async fn get_blob(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asked = asked(&host, &p, &headers).await?;
    let cid = param(&p, "cid")?;
    let cid = Cid::parse(cid).map_err(|e| invalid(format!("cid: {e}")))?;
    let not_found = || {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "BlobNotFound",
            format!("no blob {cid} in this repo"),
        )
    };
    // Whether a blob no record here references exists is not the reader's.
    let did = &asked.account.did;
    let referenced = host.store.space_has_blob(did, &asked.space, &cid);
    if !referenced.map_err(XrpcError::internal)? {
        return Err(not_found());
    }
    let found = host.read_blob(did, &cid).map_err(XrpcError::internal)?;
    let (blob, bytes) = found.ok_or_else(not_found)?;
    let headers = [
        (header::CONTENT_TYPE, blob.mime_type),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox".to_string(),
        ),
    ];
    Ok(private((headers, bytes).into_response(), None))
}

pub async fn list_blobs(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let asked = asked(&host, &p, &headers).await?;
    let limit = page(&p, 500, 1000)?;
    let (since, cursor) = (
        p.get("since").map(String::as_str),
        p.get("cursor").map(String::as_str),
    );
    let cids = host
        .store
        .space_blobs(&asked.account.did, &asked.space, since, cursor, limit)
        .map_err(XrpcError::internal)?;
    let cursor = cids
        .last()
        .filter(|_| cids.len() == limit as usize)
        .map(ToString::to_string);
    let cids: Vec<String> = cids.iter().map(ToString::to_string).collect();
    Ok(private(
        Json(json!({"cids": cids, "cursor": cursor})).into_response(),
        None,
    ))
}

/// The claims of the request's service auth (`Authorization: Bearer`),
/// signed by its issuer's key and for `lxm`.
async fn service_caller(host: &Host, headers: &HeaderMap, lxm: &str) -> Result<Claims, XrpcError> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|a| a.strip_prefix("Bearer "));
    let token = bearer.ok_or_else(|| unauthorized("AuthMissing", "service auth is required"))?;
    let bad = |e: service_auth::AuthError| unauthorized("BadJwt", e.0);
    let (_, claims) = service_auth::parts(token).map_err(bad)?;
    let check = |key: &atproto::PublicKey| service_auth::verify(token, key, now());
    let checked = host.identity.with_key(claims.account(), check).await;
    let checked = checked
        .ok_or_else(|| unauthorized("BadJwtIss", format!("no signing key for {}", claims.iss)))?;
    let claims = checked.map_err(bad)?;
    if claims.lxm.as_deref() != Some(lxm) {
        return Err(unauthorized(
            "BadJwtLexiconMethod",
            format!("a token not for {lxm}"),
        ));
    }
    Ok(claims)
}

fn forbidden(message: &str) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

/// A space's authority says credentials it issued are revoked: they read
/// no repo here until they would have lapsed anyway.
pub async fn notify_credential_revoked(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, XrpcError> {
    let claims = service_caller(&host, &headers, NOTIFY_REVOKED).await?;
    let space = body["space"].as_str().unwrap_or_default();
    let space = SpaceRef::parse(space).map_err(|e| invalid(e.to_string()))?;
    let jtis: Option<Vec<String>> = body["credentials"].as_array().and_then(|c| {
        c.iter()
            .map(|j| j.as_str().filter(|j| !j.is_empty()).map(String::from))
            .collect()
    });
    let jtis = jtis
        .filter(|j| (1..=100).contains(&j.len()))
        .ok_or_else(|| invalid("credentials are 1 to 100 credentials' jti"))?;
    if claims.account() != space.authority {
        return Err(forbidden("revocation issuer is not the space authority"));
    }
    let hosted = host
        .store
        .account(&claims.aud)
        .map_err(XrpcError::internal)?;
    if !hosted.is_some_and(|a| a.did == claims.aud) {
        return Err(forbidden("revocation audience is not a repo hosted here"));
    }
    let until = now() + CREDENTIAL_MAX_AGE + 2 * CLOCK_SKEW;
    host.store
        .revoke(&space.to_string(), &jtis, until as i64, now() as i64)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
