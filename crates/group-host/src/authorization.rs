//! Signing an app in as a group, from the member's side: the console's
//! approval page reads the request, the member picks the group, and
//! approves or declines. The member is whoever signed the service auth,
//! their own PDS vouching for them, so who signed an app in is never a
//! guess, and the grant is theirs: it ends when they are put out.

use crate::Host;
use crate::api::{Params, param};
use crate::assistants;
use crate::group::meta_space;
use crate::group_api::{group_of, signed_in};
use crate::oauth_scope::{concrete, configure};
use crate::oauth_store::Request;
use crate::rbac::{Access, NS};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn expired() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "ExpiredRequest",
        "this sign-in request has expired; go back to the app and try again",
    )
}

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

fn pending(host: &Host, request_uri: &str) -> Result<Request, XrpcError> {
    let found = host
        .pending_request(request_uri)
        .map_err(XrpcError::internal)?;
    found.ok_or_else(expired)
}

fn requested(request: &Request) -> Vec<String> {
    let scope = request.parameters["scope"].as_str().unwrap_or_default();
    scope.split(' ').map(String::from).collect()
}

fn objects(field: &str, items: &[String]) -> Value {
    items.iter().map(|i| json!({ field: i })).collect()
}

impl Host {
    /// The group an app named in its `login_hint`, by handle or DID, if
    /// it is one here and switched on.
    fn named_group(&self, request: &Request) -> Result<Option<String>, XrpcError> {
        let Some(hint) = request.parameters["login_hint"].as_str() else {
            return Ok(None);
        };
        let hint = hint.trim_start_matches("at://").trim_start_matches('@');
        let account = self.store.account(&hint.to_lowercase());
        let did = account.map_err(XrpcError::internal)?.map(|a| a.did);
        let did = did.filter(|d| group_of(self, d).is_ok() && self.active(d).is_ok());
        Ok(did)
    }

    /// The groups here, switched on, where `member` holds a role.
    pub(crate) fn groups_of(&self, member: &str) -> Result<Vec<String>, XrpcError> {
        let accounts = self.store.accounts(None, u32::MAX);
        let mut groups = Vec::new();
        for (did, _) in accounts.map_err(XrpcError::internal)? {
            let roles = self.roles(&did, member).map_err(XrpcError::internal)?;
            if group_of(self, &did).is_ok() && self.active(&did).is_ok() && !roles.is_empty() {
                groups.push(did);
            }
        }
        Ok(groups)
    }

    /// A group as the approval page shows it to `member`: what of its
    /// profile they may read, their roles, and what the app would get.
    pub(crate) fn offered(&self, group: &str, member: &str, requested: &[String]) -> Answer {
        let handle = group_of(self, group)?;
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let granted = match roles.is_empty() {
            true => Vec::new(),
            false => self.narrow(group, &roles, member, requested)?,
        };
        let withheld: Vec<String> = requested
            .iter()
            .filter(|s| !granted.contains(s))
            .cloned()
            .collect();
        let mut view = json!({
            "did": group,
            "handle": handle,
            "roles": objects("role", &roles),
            "granted": objects("scope", &granted),
            "withheld": objects("scope", &withheld),
        });
        let (meta, access) = (meta_space(group), format!("{NS}.access"));
        let access = self.group_record(group, &meta, &access, "self");
        let access = access.map_err(XrpcError::internal)?.unwrap_or_default();
        if Access::of(&access, &meta).can_read(&roles, member) {
            let profile = self.group_record(group, &meta, &format!("{NS}.profile"), "self");
            let profile = profile.map_err(XrpcError::internal)?.unwrap_or_default();
            if let Some(name) = profile.get("displayName") {
                view["displayName"] = name.clone();
            }
            if let Some(cid) = profile["avatar"]["ref"]["$link"].as_str() {
                // Read as any file in the space is, by whoever may.
                let url = reqwest::Url::parse_with_params(
                    &format!("{}/xrpc/{NS}.getSpaceBlob", self.config.public_url),
                    &[
                        ("group", group),
                        ("space", &meta),
                        ("repo", group),
                        ("cid", cid),
                    ],
                );
                view["avatar"] = json!(url.expect("the public URL is a URL").as_str());
            }
        }
        Ok(Json(view))
    }
}

/// What the configure permission set says it is for.
fn labels(requested: &[String]) -> Value {
    let lexicon: Value = serde_json::from_str(include_str!(
        "../../../lexicons/fyi/opensocial/configure.json"
    ))
    .expect("the configure lexicon is JSON");
    let text = &lexicon["defs"]["main"]["detail"];
    let ours = requested.iter().filter(|s| **s == configure());
    ours.map(|s| json!({"scope": s, "text": text})).collect()
}

/// An app's pending request to sign in as a group, as the caller would
/// approve it: the app, and the groups they may sign it in as, each with
/// what their roles there would let it do.
pub async fn get_authorization(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.getAuthorization")).await?;
    let uri = param(&p, "requestUri")?;
    if uri.starts_with(assistants::PREFIX) {
        return Ok(Json(assistants::assistant_view(&host, uri, &me)?));
    }
    let request = pending(&host, uri)?;
    let requested = requested(&request);
    let metadata = host.client(&request.client_id, now_millis()).await;
    let metadata = metadata.map(|c| c.metadata).unwrap_or_default();
    let mut client = json!({"clientId": request.client_id});
    for (k, from) in [
        ("name", "client_name"),
        ("uri", "client_uri"),
        ("logo", "logo_uri"),
    ] {
        if let Some(v) = metadata[from].as_str() {
            client[k] = json!(v);
        }
    }
    let groups = match host.named_group(&request)? {
        Some(named) => vec![named],
        None => host.groups_of(&me)?,
    };
    let mut offered = Vec::new();
    for group in &groups {
        offered.push(host.offered(group, &me, &requested)?.0);
    }
    let mut out = json!({
        "client": client,
        "requested": objects("scope", &requested),
        "labels": labels(&requested),
        "groups": offered,
    });
    if let Some(hint) = request.parameters.get("login_hint") {
        out["loginHint"] = hint.clone();
    }
    Ok(Json(out))
}

/// Approves an app's pending request to sign in as a group: it gets what
/// the caller's roles there allow, and the grant is theirs.
pub async fn approve_authorization(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.approveAuthorization")).await?;
    let uri = body["requestUri"].as_str();
    let uri = uri.expect("its lexicon requires it");
    let group = body["group"].as_str();
    let group = group.expect("its lexicon requires it");
    if uri.starts_with(assistants::PREFIX) {
        let redirect = assistants::approve(&host, uri, &me, group)?;
        return Ok(Json(json!({ "redirectUrl": redirect })));
    }
    let request = pending(&host, uri)?;
    group_of(&host, group)?;
    host.active(group)?;
    let named = host.named_group(&request)?;
    if named.is_some_and(|n| n != group) {
        return Err(forbidden("the app asked to sign in as another group"));
    }
    // A group has no password: without a role there is nothing to act with.
    let roles = host.roles(group, &me).map_err(XrpcError::internal)?;
    if roles.is_empty() {
        return Err(forbidden("you hold no role in this group"));
    }
    let granted = host.narrow(group, &roles, &me, &requested(&request))?;
    let scope = concrete(&granted, group);
    let approved = host.approve_request(&request, group, &me, &scope);
    let redirect = approved.map_err(XrpcError::internal)?.ok_or_else(expired)?;
    tracing::info!(
        member = me,
        client = request.client_id,
        group,
        scope,
        "approved"
    );
    Ok(Json(json!({ "redirectUrl": redirect })))
}

/// Declines an app's pending request: the browser goes back to it with
/// access_denied.
pub async fn decline_authorization(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    signed_in(&host, &headers, &format!("{NS}.declineAuthorization")).await?;
    let uri = body["requestUri"].as_str();
    let uri = uri.expect("its lexicon requires it");
    if uri.starts_with(assistants::PREFIX) {
        let redirect = assistants::decline(&host, uri)?;
        return Ok(Json(json!({ "redirectUrl": redirect })));
    }
    let request = pending(&host, uri)?;
    let declined = host.decline_request(&request);
    let redirect = declined.map_err(XrpcError::internal)?.ok_or_else(expired)?;
    Ok(Json(json!({ "redirectUrl": redirect })))
}
