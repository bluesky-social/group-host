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
use std::sync::Arc;

type Params = Query<HashMap<String, String>>;
type Answer = Result<Json<Value>, XrpcError>;

fn invalid(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidRequest", message)
}

/// The parameter `name`, which the method needs.
fn param<'a>(params: &'a HashMap<String, String>, name: &str) -> Result<&'a str, XrpcError> {
    params
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| invalid(format!("{name} is required")))
}

/// The page size `limit` asks for: 1 to 100, 50 unless it says.
fn limit(params: &HashMap<String, String>) -> Result<u32, XrpcError> {
    match params.get("limit") {
        None => Ok(50),
        Some(l) => l
            .parse()
            .ok()
            .filter(|n| (1..=100).contains(n))
            .ok_or_else(|| invalid("limit is 1 to 100")),
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
fn did_of(host: &Host, repo: &str) -> Result<String, XrpcError> {
    let account = host.store.account(repo).map_err(XrpcError::internal)?;
    account.map(|a| a.did).ok_or_else(|| repo_not_found(repo))
}

fn view(did: &str, r: &Record) -> Value {
    let value = drisl::decode(&r.value)
        .expect("a record the host wrote")
        .to_json();
    json!({"uri": format!("at://{did}/{}/{}", r.collection, r.rkey), "cid": r.cid.to_string(), "value": value})
}

pub async fn describe_repo(State(host): State<Arc<Host>>, Query(p): Params) -> Answer {
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

pub async fn get_record(State(host): State<Arc<Host>>, Query(p): Params) -> Answer {
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

pub async fn list_records(State(host): State<Arc<Host>>, Query(p): Params) -> Answer {
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
    Query(p): Params,
) -> Result<Response, XrpcError> {
    let did = param(&p, "did")?;
    let car = host.export(did).map_err(XrpcError::internal)?;
    let car = car.ok_or_else(|| repo_not_found(did))?;
    Ok(([(header::CONTENT_TYPE, "application/vnd.ipld.car")], car).into_response())
}

pub async fn get_latest_commit(State(host): State<Arc<Host>>, Query(p): Params) -> Answer {
    let did = param(&p, "did")?;
    let head = host.store.head(did).map_err(XrpcError::internal)?;
    let head = head.ok_or_else(|| repo_not_found(did))?;
    Ok(Json(json!({"cid": head.cid.to_string(), "rev": head.rev})))
}

pub async fn list_repos(State(host): State<Arc<Host>>, Query(p): Params) -> Answer {
    let limit = limit(&p)?;
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
        .map(|(did, head)| json!({"did": did, "head": head.cid.to_string(), "rev": head.rev, "active": true}))
        .collect();
    Ok(Json(json!({"repos": repos, "cursor": cursor})))
}

pub async fn resolve_handle(State(host): State<Arc<Host>>, Query(p): Params) -> Answer {
    let handle = param(&p, "handle")?;
    let account = host.store.account(handle).map_err(XrpcError::internal)?;
    let account = account.filter(|a| a.handle == handle);
    let not_found = || {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "HandleNotFound",
            format!("no account {handle} here"),
        )
    };
    Ok(Json(json!({"did": account.ok_or_else(not_found)?.did})))
}

/// The host's own DID document, when its DID is a `did:web` of its address.
pub async fn did_json(State(host): State<Arc<Host>>) -> Result<Json<Value>, StatusCode> {
    let did = &host.config.service_did;
    if !did.starts_with("did:web:") {
        return Err(StatusCode::NOT_FOUND);
    }
    let services = [(
        "#atproto_pds",
        "AtprotoPersonalDataServer",
        host.config.public_url.as_str(),
    )];
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
    let account = host
        .store
        .account(&handle)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    account
        .filter(|a| a.handle == handle)
        .map(|a| a.did)
        .ok_or(StatusCode::NOT_FOUND)
}
