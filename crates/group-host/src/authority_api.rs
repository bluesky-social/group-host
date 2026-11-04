//! The `com.atproto.space.*` methods a space's authority answers, for the
//! spaces of the groups here: issuing credentials, hearing of writes,
//! listing the repos written, and registering the services told of them.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::authority::Access;
use crate::space::NOTIFY_WRITE;
use crate::space_api::{credential, forbidden, now, refused, service_caller};
use crate::store::Space;
use crate::time::iso;
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use atproto::space_auth::{self, Expect, Kind, Make, space_host_aud};
use atproto::{PublicKey, Value as Drisl, did, syntax, tid};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

/// How long a service is told of a space's writes once it registers.
const REGISTERED_FOR: u64 = 24 * 60 * 60;
/// How far ahead of the host's clock a revision may be.
const FUTURE_REV: u64 = 5 * 60;

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

/// A space credential for the body's `credentialKid`, a P-256 key of the
/// caller's: to the user a `delegationToken` names, if the space lets them
/// read it, or to anyone, without one, for a space anyone may read. A user
/// it does not let read is answered as if there were no such space.
pub async fn get_space_credential(
    State(host): State<Arc<Host>>,
    Json(body): Json<Value>,
) -> Answer {
    let uri = text(&body, "space")?;
    let space = space_ref(uri)?;
    let kid = text(&body, "credentialKid")?;
    if !matches!(PublicKey::from_did_key(kid), Ok(PublicKey::P256(_))) {
        return Err(invalid("credentialKid is a P-256 did:key"));
    }
    let delegation = match body["delegationToken"].as_str() {
        Some(token) => Some(delegated(&host, token, &space).await?),
        None => None,
    };
    let user = delegation.as_ref().map(|d| d.iss.as_str());
    let hosted = host.store.space(uri).map_err(XrpcError::internal)?;
    let hosted = hosted.ok_or_else(not_found)?;
    let may = host.may(uri, &hosted, user, Access::Read);
    if !may.map_err(XrpcError::internal)? {
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
    let authority = host
        .store
        .account(&hosted.authority)
        .map_err(XrpcError::internal)?;
    let authority = authority.ok_or_else(not_found)?;
    let make = Make {
        iss: &authority.did,
        sub: uri,
        key_id: Some(kid),
        ..Make::default()
    };
    let credential = space_auth::create(Kind::Credential, &make, &authority.signing, now())
        .map_err(XrpcError::internal)?;
    // Kept, so a member who loses the space has theirs revoked.
    if let Some(user) = user {
        let (_, issued) = space_auth::parse(Kind::Credential, &credential).map_err(refused)?;
        host.store
            .issued(uri, user, &issued.jti, issued.exp as i64, now() as i64)
            .map_err(XrpcError::internal)?;
    }
    Ok(Json(json!({ "credential": credential })))
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
pub async fn notify_write(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let caller = service_caller(&host, &headers, NOTIFY_WRITE).await?;
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
        return Err(forbidden("notifyWrite's issuer is not the writer"));
    }
    if caller.aud != space.authority && caller.aud != space_host_aud(&space.authority) {
        return Err(forbidden("notifyWrite is not for this space's authority"));
    }
    let hosted = host.store.space(uri).map_err(XrpcError::internal)?;
    let hosted = hosted.ok_or_else(not_found)?;
    let ahead = tid::micros(rev).unwrap_or_default() / 1_000_000;
    if ahead > now() + FUTURE_REV {
        return Err(bad("FutureRev", "repoRev is in the future"));
    }
    let may = host.may(uri, &hosted, Some(repo), Access::Write);
    if !may.map_err(XrpcError::internal)? {
        return Err(forbidden("the writer may not write in this space"));
    }
    let kept = host.record_writer(uri, &hosted.authority, repo, rev, &data);
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

/// The repos written in a space, in the order the space last heard of
/// each: who syncs it reads each from its host.
pub async fn list_repos(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let uri = param(&p, "space")?;
    as_authority(&host, &headers, uri).await?;
    let limit = page(&p, 100, 1000)?;
    let cursor = p.get("cursor").map(String::as_str);
    let writers = host
        .store
        .writers(uri, cursor, limit)
        .map_err(XrpcError::internal)?;
    let cursor = writers.last().map(|w| w.space_rev.clone());
    let repos: Vec<Value> = writers
        .iter()
        .map(|w| {
            json!({
                "did": w.did,
                "repoRev": w.repo_rev,
                "data": Drisl::Bytes(w.data.clone()).to_json(),
                "spaceRev": w.space_rev,
            })
        })
        .collect();
    Ok(Json(json!({"repos": repos, "cursor": cursor})))
}

/// A service that syncs a space is told of its writes, for a day, at the
/// endpoint its own DID document names.
pub async fn register_notify(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let uri = text(&body, "space")?;
    let service = text(&body, "service")?;
    as_authority(&host, &headers, uri).await?;
    let (who, fragment) = service.split_once('#').unwrap_or((service, "atproto_pds"));
    let doc = host.identity.document(who).await.ok();
    let endpoint = doc.and_then(|d| did::service(&d, &format!("#{fragment}")));
    let endpoint = endpoint.ok_or_else(|| {
        bad(
            "ServiceNotResolvable",
            format!("no service endpoint for {service}"),
        )
    })?;
    let until = now() + REGISTERED_FOR;
    host.store
        .register(uri, service, &endpoint, until as i64)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({ "expiresAt": iso(until as i64 * 1000) })))
}

/// A service is told of a space's writes no more.
pub async fn unregister_notify(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let uri = text(&body, "space")?;
    let service = text(&body, "service")?;
    as_authority(&host, &headers, uri).await?;
    host.store
        .unregister(uri, service)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
