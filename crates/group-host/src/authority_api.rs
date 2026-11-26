//! The `com.atproto.space.*` methods a space's authority answers, for the
//! spaces of the groups here: authorizing a syncer, with a credential and
//! a registration to be told of changes; hearing of writes; and listing
//! the changes to the writer set.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::authority::{Access, repo_removed, repo_write};
use crate::space::NOTIFY_REPO_UPDATE;
use crate::space_api::{credential, forbidden, now, refused, service_caller};
use crate::store::{Registration, Space};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use atproto::space_auth::{self, Expect, Kind, Make, space_host_aud};
use atproto::{PublicKey, Value as Drisl, syntax, tid};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use reqwest::Url;
use serde_json::{Value, json};
use std::sync::Arc;

/// How far ahead of the host's clock a revision may be.
const FUTURE_REV: u64 = 5 * 60;
/// The longest token a syncer registers with, in bytes.
const TOKEN_AT_MOST: usize = 512;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

fn not_found() -> XrpcError {
    bad("SpaceNotFound", "space not found")
}

/// The space `uri` names, written as a space is.
fn space_ref(uri: &str) -> Result<SpaceRef, XrpcError> {
    SpaceRef::parse(uri).map_err(|e| invalid(e.to_string()))
}

fn text<'a>(body: &'a Value, name: &str) -> Result<&'a str, XrpcError> {
    body[name]
        .as_str()
        .ok_or_else(|| invalid(format!("{name} is required")))
}

/// The endpoint and token a body's `notify` registers, if it has one: an
/// https URL (or http, while this host is on http itself, as a network of
/// test servers is), and a token of at most 512 bytes.
fn notify_of(host: &Host, body: &Value) -> Result<Option<Registration>, XrpcError> {
    let notify = &body["notify"];
    if notify.is_null() {
        return Ok(None);
    }
    let (endpoint, token) = (text(notify, "endpoint")?, text(notify, "token")?);
    let http_here = host.config.public_url.starts_with("http://");
    let url = Url::parse(endpoint).ok();
    if !url.is_some_and(|u| u.scheme() == "https" || (http_here && u.scheme() == "http")) {
        return Err(invalid("notify's endpoint is an https URL"));
    }
    if token.len() > TOKEN_AT_MOST {
        return Err(invalid("notify's token is at most 512 bytes"));
    }
    Ok(Some(Registration {
        endpoint: endpoint.into(),
        token: token.into(),
    }))
}

/// A space credential for the body's `credentialKey`, a P-256 key of the
/// caller's, and the space's revision: to the user a `delegationToken`
/// names, if the space lets them read it, or to anyone, without one, for a
/// space anyone may read. A user it does not let read is answered as if
/// there were no such space, and the registration they ask for withdrawn.
/// With `notify`, the endpoint it names is told of the space's changes
/// while the credential lasts, once it confirms it asked to be.
pub async fn authorize_space_sync(
    State(host): State<Arc<Host>>,
    Json(body): Json<Value>,
) -> Answer {
    let uri = text(&body, "space")?;
    let space = space_ref(uri)?;
    let key = text(&body, "credentialKey")?;
    if !matches!(PublicKey::from_did_key(key), Ok(PublicKey::P256(_))) {
        return Err(invalid("credentialKey is a P-256 did:key"));
    }
    let notify = notify_of(&host, &body)?;
    let authority = host
        .store
        .account(&space.authority)
        .map_err(XrpcError::internal)?;
    let authority = authority
        .filter(|a| a.did == space.authority)
        .ok_or_else(not_found)?;
    let off = host.store.deactivation(&authority.did);
    if off.map_err(XrpcError::internal)?.is_some() {
        return Err(bad(
            "RepoDeactivated",
            "the space's authority is switched off",
        ));
    }
    // Whoever asks: a syncer that missed the deletion learns of it here.
    let deleted = host.store.space_deleted(uri).map_err(XrpcError::internal)?;
    if deleted {
        return Err(bad("SpaceDeleted", "the space was deleted"));
    }
    let delegation = match body["delegationToken"].as_str() {
        Some(token) => Some(delegated(&host, token, &space).await?),
        None => None,
    };
    // `clientAttestation` is taken and neither checked nor used: no space
    // here admits by the app asking.
    let user = delegation.as_ref().map(|d| d.iss.as_str());
    let hosted = host.store.space(uri).map_err(XrpcError::internal)?;
    let hosted = hosted.ok_or_else(not_found)?;
    // A delegation a group's grant was given here buys only while the
    // grant lasts and its member reads the space.
    let grant = match &delegation {
        Some(d) => host.store.delegation_grant(&d.jti),
        None => Ok(None),
    };
    let grant = grant.map_err(XrpcError::internal)?;
    let may = host.may(uri, &hosted, user, Access::Read);
    let mut may = may.map_err(XrpcError::internal)?;
    if let Some(id) = grant.as_deref().filter(|_| may) {
        let reads = host.grant_reads(id, uri, &hosted);
        may = reads.map_err(XrpcError::internal)?;
    }
    if !may {
        // A syncer that lost the space is told of it no more.
        if let Some(to) = &notify {
            let gone = host.store.unregister(uri, to);
            gone.map_err(XrpcError::internal)?;
        }
        return Err(not_found());
    }
    // Spent once it buys a credential: one token, one credential.
    if let Some(d) = &delegation {
        let spent = host.store.spend(&d.iss, &d.jti, d.exp as i64, now() as i64);
        if !spent.map_err(XrpcError::internal)? {
            return Err(bad(
                "InvalidDelegationToken",
                "the delegation token was used already",
            ));
        }
    }
    // An endpoint registered already, lapsed or not, confirmed it then.
    if let Some(to) = &notify {
        let registered = host.store.registration(uri, to);
        if registered.map_err(XrpcError::internal)?.is_none() {
            host.confirm_registration(uri, to).await?;
        }
    }
    let make = Make {
        iss: &authority.did,
        sub: uri,
        key_id: Some(key),
        ..Make::default()
    };
    let credential = space_auth::create(Kind::Credential, &make, &authority.signing, now())
        .map_err(XrpcError::internal)?;
    let (_, issued) = space_auth::parse(Kind::Credential, &credential).map_err(refused)?;
    // Kept, so a member who loses the space has theirs revoked.
    if let Some(user) = user {
        host.store
            .issued(uri, user, &issued.jti, issued.exp as i64, now() as i64)
            .map_err(XrpcError::internal)?;
    }
    if let Some(id) = &grant {
        let kept = host.store.bought(uri, &issued.jti, id);
        kept.map_err(XrpcError::internal)?;
    }
    // Renewed with each credential, so a syncer that keeps authorizing
    // stays registered.
    if let Some(to) = &notify {
        let kept = host.store.register(uri, to, issued.exp as i64);
        kept.map_err(XrpcError::internal)?;
        let bound = host.store.bind_registration(uri, to, grant.as_deref());
        bound.map_err(XrpcError::internal)?;
    }
    // Read once registered, so that any change after it is told.
    let space_rev = host.store.space_rev(uri).map_err(XrpcError::internal)?;
    let mut answer = json!({ "credential": credential });
    if let Some(rev) = space_rev {
        answer["spaceRev"] = json!(rev);
    }
    Ok(Json(answer))
}

/// What a delegation token says, once it is signed by the user it names,
/// for this space's authority and this space.
async fn delegated(
    host: &Host,
    token: &str,
    space: &SpaceRef,
) -> Result<space_auth::Token, XrpcError> {
    let refused = |m: String| bad("InvalidDelegationToken", m);
    let (_, claims) = space_auth::parse(Kind::Delegation, token).map_err(|e| refused(e.message))?;
    let aud = space_host_aud(&space.authority);
    let uri = space.to_string();
    let expect = Expect {
        aud: Some(&aud),
        sub: Some(&uri),
    };
    let check = |key: &PublicKey| space_auth::verify(Kind::Delegation, token, key, &expect, now());
    let checked = host.identity.with_key(&claims.iss, check).await;
    let checked = checked.ok_or_else(|| refused(format!("no signing key for {}", claims.iss)))?;
    checked.map_err(|e| refused(e.message))
}

/// A repo host says a repo of one of its accounts was written in a space
/// of a group here: kept in the space's writer set if the space lets the
/// account write, and told to the space's syncers.
pub async fn notify_repo_update(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let caller = service_caller(&host, &headers, NOTIFY_REPO_UPDATE).await?;
    let uri = text(&body, "space")?;
    let space = space_ref(uri)?;
    let (repo, rev) = (text(&body, "repo")?, text(&body, "repoRev")?);
    if !syntax::is_tid(rev) {
        return Err(invalid("repoRev is a TID"));
    }
    let data = Drisl::from_json(&body["data"]);
    let Ok(Drisl::Bytes(data)) = data else {
        return Err(invalid("data is bytes"));
    };
    // A repo host speaks for its own accounts alone.
    if caller.account() != repo {
        return Err(forbidden("notifyRepoUpdate's issuer is not the repo"));
    }
    if caller.aud != space.authority && caller.aud != space_host_aud(&space.authority) {
        return Err(forbidden(
            "notifyRepoUpdate is not for this space's authority",
        ));
    }
    let hosted = host.store.space(uri).map_err(XrpcError::internal)?;
    let hosted = hosted.ok_or_else(not_found)?;
    let ahead = tid::micros(rev).unwrap_or_default() / 1_000_000;
    if ahead > now() + FUTURE_REV {
        return Err(bad("FutureRev", "repoRev is in the future"));
    }
    let may = host.may(uri, &hosted, Some(repo), Access::Write);
    if !may.map_err(XrpcError::internal)? {
        return Err(forbidden("the repo may not write in this space"));
    }
    let kept = host.record_writer(uri, repo, rev, &data);
    kept.map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// The space a credential to it names, if its authority is here: the
/// credential for the authority, not a repo.
async fn as_authority(host: &Host, headers: &HeaderMap, uri: &str) -> Result<Space, XrpcError> {
    let space = space_ref(uri)?;
    credential(host, headers, uri, &space.authority).await?;
    host.hosted_space(uri)
}

/// The changes to a space's writer set, in its order: the last for each
/// repo, a write or its removal. Who syncs it reads each repo written from
/// its host.
pub async fn list_writers(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let uri = param(&p, "space")?;
    as_authority(&host, &headers, uri).await?;
    let limit = page(&p, 100, 1000)?;
    let cursor = p.get("cursor").map(String::as_str);
    let changes = host
        .store
        .writer_log(uri, cursor, limit)
        .map_err(XrpcError::internal)?;
    let repos: Vec<Value> = changes
        .iter()
        .map(|c| {
            let (w, prev) = (&c.writer, c.prev_space_rev.as_deref());
            match c.removed {
                true => repo_removed(&w.did, &w.space_rev, prev),
                false => repo_write(&w.did, &w.repo_rev, &w.data, &w.space_rev, prev),
            }
        })
        .collect();
    let mut answer = json!({ "repos": repos });
    if let Some(last) = changes.last() {
        answer["cursor"] = json!(last.writer.space_rev);
    }
    Ok(Json(answer))
}
