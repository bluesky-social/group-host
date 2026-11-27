//! The `com.atproto.*` methods a PDS answers for its accounts' repos, and
//! the well-known documents that name the host and its groups.

use crate::Host;
use crate::store::Record;
use crate::xrpc::XrpcError;
use atproto::drisl;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::HashMap;

/// The most a JSON answer from another server is read to.
pub const ANSWER_AT_MOST: usize = 16 * 1024 * 1024;
/// The most a repo, or a repo in a space, brought from another server is
/// read to.
pub const REPO_AT_MOST: usize = 256 * 1024 * 1024;

/// Why an answer read up to `max` bytes was not taken.
pub(crate) fn too_much(max: usize) -> String {
    format!("an answer of more than {max} bytes, or one cut off")
}
use std::sync::Arc;

pub(crate) type Params = HashMap<String, String>;
type Answer = Result<Json<Value>, XrpcError>;

pub(crate) fn invalid(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidRequest", message)
}

/// A cursor this host never gave, as a list whose lexicon names
/// `InvalidCursor` refuses it.
pub(crate) fn invalid_cursor() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "InvalidCursor",
        "a cursor this host gave",
    )
}

/// The parameter `name`, which the method needs.
pub(crate) fn param<'a>(params: &'a Params, name: &str) -> Result<&'a str, XrpcError> {
    params
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| invalid(format!("{name} is required")))
}

/// The page size `limit` asks for: 1 to 100, 50 unless it says.
fn limit(params: &Params) -> Result<u32, XrpcError> {
    page(params, 50, 100)
}

/// The page size `limit` asks for: 1 to `max`, `default` unless it says.
pub(crate) fn page(params: &Params, default: u32, max: u32) -> Result<u32, XrpcError> {
    match params.get("limit") {
        None => Ok(default),
        Some(l) => l
            .parse()
            .ok()
            .filter(|n| (1..=max).contains(n))
            .ok_or_else(|| invalid(format!("limit is 1 to {max}"))),
    }
}

fn repo_not_found(repo: &str) -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "RepoNotFound",
        format!("no repo {repo} here"),
    )
}

/// The DID of the account `repo` names, by DID or handle.
/// Refused for an account switched off.
fn did_of(host: &Host, repo: &str) -> Result<String, XrpcError> {
    let account = host.store.account(repo).map_err(XrpcError::internal)?;
    let did = account.map(|a| a.did).ok_or_else(|| repo_not_found(repo))?;
    host.active(&did)?;
    Ok(did)
}

fn view(did: &str, r: &Record) -> Value {
    let value = drisl::decode(&r.value)
        .expect("a record the host wrote")
        .to_json();
    json!({"uri": format!("at://{did}/{}/{}", r.collection, r.rkey), "cid": r.cid.to_string(), "value": value})
}

/// Whether an address under this host's domain is free for a new group:
/// no account's handle here, nor kept by a group as its address. Anyone
/// may ask, as anyone may resolve a handle.
pub async fn check_address(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Answer {
    let address = param(&p, "address")?.to_ascii_lowercase();
    let label = address.strip_suffix(&host.config.handle_domain);
    let ours = label.is_some_and(|l| !l.is_empty() && !l.contains('.'));
    if !ours {
        return Err(invalid(format!(
            "an address is one label under {}",
            host.config.handle_domain
        )));
    }
    let available = !host.held_here(&address)?;
    Ok(Json(json!({ "address": address, "available": available })))
}

/// The host as a PDS describes itself: its DID, and the domain whose names
/// it gives its groups. Nobody signs up here: a group is made for someone.
pub async fn describe_server(State(host): State<Arc<Host>>) -> Json<Value> {
    Json(json!({
        "did": host.config.service_did,
        "availableUserDomains": [host.config.handle_domain],
        "inviteCodeRequired": true,
        "blobUploadLimit": host.config.blob_upload_limit,
    }))
}

pub async fn describe_repo(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Answer {
    let repo = param(&p, "repo")?;
    let account = host.store.account(repo).map_err(XrpcError::internal)?;
    let account = account.ok_or_else(|| repo_not_found(repo))?;
    let doc = host
        .plc
        .document(&account.did)
        .await
        .map_err(XrpcError::internal)?;
    let collections = host
        .store
        .collections(&account.did)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({
        "handle": account.handle,
        "did": account.did,
        "didDoc": doc,
        "collections": collections,
        "handleIsCorrect": true,
    })))
}

pub async fn get_record(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Answer {
    let did = did_of(&host, param(&p, "repo")?)?;
    let (collection, rkey) = (param(&p, "collection")?, param(&p, "rkey")?);
    let found = host
        .store
        .record(&did, collection, rkey)
        .map_err(XrpcError::internal)?;
    // A `cid` asks for that version, which is only ever the one here.
    let found = found.filter(|r| p.get("cid").is_none_or(|c| *c == r.cid.to_string()));
    let not_found = || {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "RecordNotFound",
            format!("no record {collection}/{rkey}"),
        )
    };
    Ok(Json(view(&did, &found.ok_or_else(not_found)?)))
}

pub async fn list_records(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Answer {
    let did = did_of(&host, param(&p, "repo")?)?;
    let (collection, limit) = (param(&p, "collection")?, limit(&p)?);
    let reverse = p.get("reverse").is_some_and(|r| r == "true");
    let cursor = p.get("cursor").map(String::as_str);
    let records = host
        .store
        .list(&did, collection, cursor, limit, reverse)
        .map_err(XrpcError::internal)?;
    // A full page may have more after it.
    let cursor = records
        .last()
        .filter(|_| records.len() == limit as usize)
        .map(|r| r.rkey.clone());
    let records: Vec<Value> = records.iter().map(|r| view(&did, r)).collect();
    Ok(Json(json!({"records": records, "cursor": cursor})))
}

pub async fn get_repo(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
) -> Result<Response, XrpcError> {
    let did = &did_of(&host, param(&p, "did")?)?;
    let car = host.export(did).map_err(XrpcError::internal)?;
    Ok(([(header::CONTENT_TYPE, "application/vnd.ipld.car")], car).into_response())
}

/// A record with the proof of it, or of its absence, from the repo's
/// signed commit: a CAR, as a PDS answers it.
pub async fn get_sync_record(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
) -> Result<Response, XrpcError> {
    let did = &did_of(&host, param(&p, "did")?)?;
    let (collection, rkey) = (param(&p, "collection")?, param(&p, "rkey")?);
    let car = host.record_proof(did, collection, rkey);
    let car = car.map_err(XrpcError::internal)?;
    Ok(([(header::CONTENT_TYPE, "application/vnd.ipld.car")], car).into_response())
}

pub async fn get_latest_commit(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Answer {
    let did = &did_of(&host, param(&p, "did")?)?;
    let head = host.store.head(did).map_err(XrpcError::internal)?;
    let head = head.expect("an account has a repo");
    Ok(Json(json!({"cid": head.cid.to_string(), "rev": head.rev})))
}

/// The page size of the sync methods that list a host's repos or a repo's
/// blobs, as atproto's lexicons have it: 1 to 1000, 500 unless asked; a
/// relay crawling the host asks for 1000.
fn sync_limit(params: &Params) -> Result<u32, XrpcError> {
    page(params, 500, 1000)
}

pub async fn list_repos(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Answer {
    let limit = sync_limit(&p)?;
    let repos = host
        .store
        .accounts(p.get("cursor").map(String::as_str), limit)
        .map_err(XrpcError::internal)?;
    let cursor = repos
        .last()
        .filter(|_| repos.len() == limit as usize)
        .map(|(did, _)| did.clone());
    let repos: Vec<Value> = repos
        .iter()
        .map(|(did, head)| {
            let mut repo =
                json!({"did": did, "head": head.cid.to_string(), "rev": head.rev, "active": true});
            if host.deactivation(did).ok().flatten().is_some() {
                repo["active"] = json!(false);
                repo["status"] = json!("deactivated");
            }
            repo
        })
        .collect();
    Ok(Json(json!({"repos": repos, "cursor": cursor})))
}

/// The DID a handle names: an account's here, or for someone signed in,
/// anyone's.
pub async fn resolve_handle(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let handle = param(&p, "handle")?.to_ascii_lowercase();
    let account = host.store.account(&handle).map_err(XrpcError::internal)?;
    let mut did = account.filter(|a| a.handle == handle).map(|a| a.did);
    // Anyone else's, as a PDS resolves them, for someone signed in: the
    // host asks the handle's own server, which no stranger sends it to.
    let lxm = "com.atproto.identity.resolveHandle";
    if did.is_none()
        && crate::group_api::caller(&host, &headers, lxm)
            .await?
            .is_some()
    {
        did = host
            .identity
            .resolve_handle(&handle, &host.config.handle_resolvers)
            .await;
    }
    let not_found = || {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "HandleNotFound",
            format!("no account {handle} we can find"),
        )
    };
    Ok(Json(json!({ "did": did.ok_or_else(not_found)? })))
}

/// A blob a record in the account's public repo references, served as a
/// download: a browser neither sniffs nor runs it. A blob only records in
/// spaces hold is not the public repo's, and is fetched from its space.
pub async fn get_blob(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
) -> Result<Response, XrpcError> {
    let did = did_of(&host, param(&p, "did")?)?;
    let cid = param(&p, "cid")?;
    let cid = atproto::Cid::parse(cid).map_err(|e| invalid(format!("cid: {e}")))?;
    let public = host.store.held_publicly(&did, &cid);
    let found = match public.map_err(XrpcError::internal)? {
        true => host.read_blob(&did, &cid).map_err(XrpcError::internal)?,
        false => None,
    };
    let not_found = || {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "BlobNotFound",
            format!("no blob {cid} here"),
        )
    };
    let (blob, bytes) = found.ok_or_else(not_found)?;
    let headers = [
        (header::CONTENT_TYPE, blob.mime_type),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox".to_string(),
        ),
    ];
    Ok((headers, bytes).into_response())
}

pub async fn list_blobs(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Answer {
    let did = did_of(&host, param(&p, "did")?)?;
    let limit = sync_limit(&p)?;
    let (since, cursor) = (
        p.get("since").map(String::as_str),
        p.get("cursor").map(String::as_str),
    );
    let cids = host
        .store
        .list_blobs(&did, since, cursor, limit)
        .map_err(XrpcError::internal)?;
    let cursor = cids
        .last()
        .filter(|_| cids.len() == limit as usize)
        .map(ToString::to_string);
    let cids: Vec<String> = cids.iter().map(ToString::to_string).collect();
    Ok(Json(json!({"cids": cids, "cursor": cursor})))
}

/// The host's own DID document, when its DID is a `did:web` of its address.
pub async fn did_json(State(host): State<Arc<Host>>) -> Result<Json<Value>, StatusCode> {
    let did = &host.config.service_did;
    if atproto::did::web_host(did).is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    // Its groups' PDS, and the service an app reaches through a person's
    // PDS, by `atproto-proxy: <did>#atgrp`, to act as them here.
    let url = host.config.public_url.as_str();
    let services = [
        ("#atproto_pds", "AtprotoPersonalDataServer", url),
        ("#atgrp", "AtgrpGroupHost", url),
    ];
    Ok(Json(atproto::did::service_document(
        did,
        &host.secrets.signing.public(),
        &services,
    )))
}

/// A group's handle, as its own address answers it: the DID its account is.
pub async fn atproto_did(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
) -> Result<String, StatusCode> {
    let asked = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let handle = asked
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let account = host.store.account(&handle);
    let account = account.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    account
        .filter(|a| a.handle == handle)
        .map(|a| a.did)
        .ok_or(StatusCode::NOT_FOUND)
}
