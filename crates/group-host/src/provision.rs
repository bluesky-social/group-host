//! An app makes a group for someone and leaves with its own session on it.
//!
//! Two parties sign in. The person asking does, as for any of the host's
//! methods, with service auth from their own PDS, and becomes the group's
//! founder. The app does, as at the token endpoint: its client id and, if
//! confidential, its assertion in the body, and a DPoP proof. It must be
//! one this host makes groups for (`PROVISIONING_CLIENTS`). Everything is
//! checked before the group is made. Its session is then issued as a code
//! exchange issues one, narrowed to the founder's roles as an approval is,
//! so from then on it is an ordinary grant: refreshed at `/oauth/token`,
//! listed among the group's, revoked there.

use crate::Host;
use crate::group::Made;
use crate::group_api::{new_group, signed_in};
use crate::oauth::{OAuthError, new_tokens, token_response};
use crate::oauth_resource::Refusal;
use crate::oauth_scope::concrete;
use crate::oauth_store::{Grant, Tokens};
use crate::rbac::INTERNAL;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::jose;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// What a grant made this way is, as the group's grants list it.
pub const SOURCE: &str = "provisionGroup";

/// An OAuth failure, as this method's own: its code and why.
fn refused(e: OAuthError) -> XrpcError {
    XrpcError::new(e.status, e.error, e.description)
}

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// Makes a group for the caller, its founder, and a session on it for the
/// app that asked.
pub async fn provision_group(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Refusal> {
    let nsid = format!("{INTERNAL}.provisionGroup");
    let founder = signed_in(&host, &headers, &nsid).await?;
    let now = now_millis();
    let proof = host.dpop_proof(&headers, "POST", &format!("/xrpc/{nsid}"), None, now);
    let proof = proof.map_err(|e| host.refuse(e.error, e.description))?;
    let proof = proof.ok_or_else(|| host.refuse("invalid_dpop_proof", "DPoP proof required"))?;
    let client_id = body["clientId"].as_str().expect("its lexicon requires it");
    let client = host.client(client_id, now).await.map_err(refused)?;
    let mut form = HashMap::new();
    for (field, key) in [
        ("clientAssertionType", "client_assertion_type"),
        ("clientAssertion", "client_assertion"),
    ] {
        if let Some(v) = body[field].as_str() {
            form.insert(key.to_string(), v.to_string());
        }
    }
    let auth = host
        .authenticate_client(&client, &form, now)
        .await
        .map_err(refused)?;
    let trusted = &host.config.provisioning_clients;
    if !trusted.iter().any(|t| t == "*" || *t == client.id) {
        let said = format!("{} is not an app this host makes groups for", client.id);
        return Err(bad("UntrustedApp", said).into());
    }
    // What the app could be granted anywhere: what its metadata declares.
    let declared = client.metadata["scope"].as_str().unwrap_or_default();
    let asked = body["scope"].as_str().unwrap_or_default();
    let requested: Vec<String> = asked
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let undeclared: Vec<&str> = requested
        .iter()
        .filter(|s| !declared.split(' ').any(|d| d == s.as_str()))
        .map(String::as_str)
        .collect();
    if !requested.iter().any(|s| s == "atproto") || !undeclared.is_empty() {
        let said = match undeclared.is_empty() {
            true => "the scope must include atproto".to_string(),
            false => format!("not in the app's client metadata: {}", undeclared.join(" ")),
        };
        return Err(bad("InvalidScope", said).into());
    }

    let new = new_group(&body, founder.clone())?;
    let Made { did, meta, members } = host.create_group(&new).await?;
    let roles = host.roles(&did, &founder).map_err(XrpcError::internal)?;
    let sets = host.permission_sets(&requested).await;
    let granted = host.narrow(&did, &roles, &requested, &sets)?;
    let (access, refresh, access_expires, expires_at) = new_tokens(&auth, now);
    let tokens = Tokens {
        access_hash: &jose::sha256(&access),
        access_expires,
        refresh_hash: &jose::sha256(&refresh),
        expires_at,
    };
    let grant = Grant {
        id: format!("tok-{}", jose::random(16)),
        group: did.clone(),
        member: founder,
        client_id: client.id,
        client_auth: auth,
        scope: concrete(&granted, &did, &sets),
        dpop_jkt: proof.jkt,
        access_expires,
        expires_at,
        created_at: now,
        source: SOURCE.into(),
        used_at: None,
    };
    host.issue_grant(&grant, &tokens).map_err(refused)?;
    Ok(Json(json!({
        "did": did,
        "meta": meta,
        "members": members,
        "session": token_response(&access, &refresh, &grant),
    })))
}
