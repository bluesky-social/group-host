//! Signing an app in as a group, from the member's side: the console's
//! approval page reads the request, the member picks the group, and
//! approves or declines. The member is whoever signed the service auth,
//! their own PDS vouching for them, so who signed an app in is never a
//! guess, and the grant is theirs: it ends when they are put out.

use crate::Host;
use crate::api::{Params, param};
use crate::group::{did_rkey, meta_space};
use crate::group_api::{group_of, signed_in};
use crate::mcp;
use crate::oauth_scope::{concrete, set_of};
use crate::oauth_store::Request;
use crate::rbac::{INTERNAL, NS};
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

    /// The groups here, switched on, where `member` holds a role: those
    /// whose own members space holds a membership naming them.
    pub(crate) fn groups_of(&self, member: &str) -> Result<Vec<String>, XrpcError> {
        let path = format!("{NS}.membership/{}", did_rkey(member));
        let holding = self
            .store
            .own_space_holding(&format!("{NS}.members"), &path);
        let mut groups = Vec::new();
        for did in holding.map_err(XrpcError::internal)? {
            let roles = self.roles(&did, member).map_err(XrpcError::internal)?;
            if group_of(self, &did).is_ok() && self.active(&did).is_ok() && !roles.is_empty() {
                groups.push(did);
            }
        }
        Ok(groups)
    }

    /// A group as the approval page shows it to `member`: what of its
    /// about record they may read, their roles, and what the app would get.
    pub(crate) fn offered(&self, group: &str, member: &str, requested: &[String]) -> Answer {
        let handle = group_of(self, group)?;
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let granted = match roles.is_empty() {
            true => Vec::new(),
            false => self.narrow(group, &roles, requested)?,
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
        let meta = meta_space(group);
        if self.reads_space(&meta, &roles)? {
            let profile = self.group_record(group, &meta, &format!("{NS}.about"), "self");
            let profile = profile.map_err(XrpcError::internal)?.unwrap_or_default();
            if let Some(name) = profile.get("displayName") {
                view["displayName"] = name.clone();
            }
            if let Some(cid) = profile["avatar"]["ref"]["$link"].as_str() {
                // Read as any file in the space is, by whoever may.
                let url = reqwest::Url::parse_with_params(
                    &format!("{}/xrpc/{INTERNAL}.getSpaceBlob", self.config.public_url),
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

/// What each of this host's permission sets among `scopes` is called,
/// and says it is for, once each.
fn labels(scopes: &[&str]) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for (i, s) in scopes.iter().enumerate() {
        let set = set_of(s).filter(|_| !scopes[..i].contains(s));
        out.extend(
            set.map(|set| json!({"scope": s, "title": set["title"], "text": set["detail"]})),
        );
    }
    out.into()
}

/// An app's pending request to sign in as a group, as the caller would
/// approve it: the app, and the groups they may sign it in as, each with
/// what their roles there would let it do.
pub async fn get_authorization(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.getAuthorization")).await?;
    let uri = param(&p, "requestUri")?;
    if uri.starts_with(mcp::PREFIX) {
        return Ok(Json(mcp::mcp_view(&host, uri, &me)?));
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
    // Each set asked for, and any within it a group would give in its
    // place.
    let granted = offered
        .iter()
        .flat_map(|g| g["granted"].as_array().into_iter().flatten());
    let named: Vec<&str> = requested
        .iter()
        .map(String::as_str)
        .chain(granted.filter_map(|g| g["scope"].as_str()))
        .collect();
    let mut out = json!({
        "client": client,
        "requested": objects("scope", &requested),
        "labels": labels(&named),
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
    let me = signed_in(&host, &headers, &format!("{INTERNAL}.approveAuthorization")).await?;
    let uri = body["requestUri"].as_str();
    let uri = uri.expect("its lexicon requires it");
    let group = body["group"].as_str();
    let group = group.expect("its lexicon requires it");
    if uri.starts_with(mcp::PREFIX) {
        let redirect = mcp::approve(&host, uri, &me, group)?;
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
    let granted = host.narrow(group, &roles, &requested(&request))?;
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
    signed_in(&host, &headers, &format!("{INTERNAL}.declineAuthorization")).await?;
    let uri = body["requestUri"].as_str();
    let uri = uri.expect("its lexicon requires it");
    if uri.starts_with(mcp::PREFIX) {
        let redirect = mcp::decline(&host, uri)?;
        return Ok(Json(json!({ "redirectUrl": redirect })));
    }
    let request = pending(&host, uri)?;
    let declined = host.decline_request(&request);
    let redirect = declined.map_err(XrpcError::internal)?.ok_or_else(expired)?;
    Ok(Json(json!({ "redirectUrl": redirect })))
}
