//! Calling other services as a group, as an account's PDS lets its apps
//! call theirs: a method the host doesn't answer itself, asked of it with
//! `atproto-proxy: <did>#<service>`, is sent on to that service with
//! service auth the group signs, and its answer streamed back; and
//! `com.atproto.server.getServiceAuth` hands an app such a token to send
//! itself. Both as the reference PDS does them, for a token or an app
//! password's session acting as a group.
//!
//! A token's scope must hold the call (`Permissions::rpc`), and the
//! member who signed the app in must still hold `repo:*`, as narrowing
//! asked of them; an app password's session may call anything but chat
//! and `createAccount`, unless it may use chat. The service is found in
//! its DID's document, and reached only at a public address (or any,
//! where the host is on http itself, for test networks), never at this
//! host. The token sent names the service's DID alone as its audience,
//! the method as `lxm`, and lapses in a minute.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::app_passwords::SOURCE as APP_PASSWORD;
use crate::oauth_resource::{Acting, Refusal};
use crate::time::now_millis;
use crate::xrpc::{XrpcError, read_body};
use atproto::did;
use atproto::jose;
use atproto::service_auth::{self, Claims};
use atproto::syntax::is_did;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// How long a proxied call may take, its answer read whole.
pub const PROXIED_FOR: Duration = Duration::from_secs(60);
/// The most bytes of a service's failure read, for its error's name.
const FAILURE_AT_MOST: usize = 64 * 1024;
/// How long a token `getServiceAuth` gives may last, in seconds: an hour
/// for one method, a minute for any.
const HOUR: i64 = 3600;
const MINUTE: i64 = 60;

/// The methods reserved for managing an account directly, never proxied
/// nor given service auth for, as the reference PDS reserves them; and
/// every `com.atproto.admin.*` method.
const PROTECTED: [&str; 15] = [
    "com.atproto.identity.requestPlcOperationSignature",
    "com.atproto.identity.signPlcOperation",
    "com.atproto.identity.updateHandle",
    "com.atproto.server.activateAccount",
    "com.atproto.server.confirmEmail",
    "com.atproto.server.createAppPassword",
    "com.atproto.server.deactivateAccount",
    "com.atproto.server.getAccountInviteCodes",
    "com.atproto.server.getSession",
    "com.atproto.server.listAppPasswords",
    "com.atproto.server.requestAccountDelete",
    "com.atproto.server.requestEmailConfirmation",
    "com.atproto.server.requestEmailUpdate",
    "com.atproto.server.revokeAppPassword",
    "com.atproto.server.updateEmail",
];

/// The request headers sent on, besides `x-atproto-*` and, for a
/// procedure, its body's `content-type` and `content-encoding`.
const SENT_ON: [&str; 3] = [
    "accept-language",
    "atproto-accept-labelers",
    "x-bsky-topics",
];

/// The answer's headers passed back: its content's, and these.
const PASSED_BACK: [&str; 3] = [
    "atproto-repo-rev",
    "atproto-content-labelers",
    "retry-after",
];
const CONTENT: [&str; 4] = [
    "content-length",
    "content-encoding",
    "content-type",
    "content-language",
];

/// XRPC's names for a failure's status, where a service names none.
const NAMES: [(u16, &str); 13] = [
    (400, "InvalidRequest"),
    (401, "AuthenticationRequired"),
    (403, "Forbidden"),
    (404, "XRPCNotSupported"),
    (406, "NotAcceptable"),
    (413, "PayloadTooLarge"),
    (415, "UnsupportedMediaType"),
    (429, "RateLimitExceeded"),
    (500, "InternalServerError"),
    (501, "MethodNotImplemented"),
    (502, "UpstreamFailure"),
    (503, "NotEnoughResources"),
    (504, "UpstreamTimeout"),
];

/// Whether `lxm` is never proxied nor given service auth for.
pub fn protected(lxm: &str) -> bool {
    let lxm = lxm.to_lowercase();
    lxm.starts_with("com.atproto.admin.") || PROTECTED.iter().any(|p| p.to_lowercase() == lxm)
}

/// Whether `lxm` needs an app password that may use chat.
pub fn privileged(lxm: &str) -> bool {
    let lxm = lxm.to_lowercase();
    lxm.starts_with("chat.bsky.") || lxm == "com.atproto.server.createaccount"
}

fn bad_method() -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidToken", "Bad token method")
}

fn upstream(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_GATEWAY, "UpstreamFailure", message)
}

/// A token whose scope doesn't hold calling `lxm` at `aud`, refused as
/// the reference PDS refuses it, naming the scope it needs.
pub fn scope_missing(lxm: &str, aud: &str) -> XrpcError {
    let scope = format!("rpc:{lxm}?aud={}", aud.replace('#', "%23"));
    let said = format!("Missing required scope \"{scope}\"");
    XrpcError::new(StatusCode::FORBIDDEN, "ScopeMissingError", said)
}

/// The DID and service id `atproto-proxy` names: `<did>#<service>`.
pub fn proxy_target(header: &str) -> Result<(&str, &str), XrpcError> {
    let Some((did, service)) = header.split_once('#') else {
        return Err(invalid("no service id specified in proxy header"));
    };
    if did.is_empty() {
        return Err(invalid("no did specified in proxy header"));
    }
    if service.is_empty() {
        return Err(invalid("no service id specified in proxy header"));
    }
    if service.contains('#') {
        return Err(invalid("invalid proxy header format"));
    }
    if header.contains(' ') {
        return Err(invalid("proxy header cannot contain spaces"));
    }
    Ok((did, service))
}

impl Host {
    /// Whether `acting` may call `lxm` (`*`: any) at `aud` as its group:
    /// for an app password's session, a method it is privileged enough
    /// for, else `privileged` says why not; for a token, one its scope
    /// holds, its member still holding `repo:*`.
    fn may_call(
        &self,
        acting: &Acting,
        lxm: &str,
        aud: &str,
        privileged_refusal: impl FnOnce() -> XrpcError,
    ) -> Result<(), XrpcError> {
        let g = &acting.grant;
        if g.source == APP_PASSWORD {
            let chat = g.scope.split(' ').any(|s| s == "transition:chat.bsky");
            return match privileged(lxm) && !chat {
                true => Err(privileged_refusal()),
                false => Ok(()),
            };
        }
        if !acting.permissions.rpc(lxm, aud) {
            return Err(scope_missing(lxm, aud));
        }
        if !self.may_issue(&g.group, &g.member)? {
            let said =
                "the member who signed this app in may no longer call other services as the group";
            return Err(XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", said));
        }
        Ok(())
    }

    /// Where `did`'s `#service` is: from its document, never this host.
    async fn endpoint(&self, did: &str, service: &str) -> Result<String, XrpcError> {
        if did == self.config.service_did {
            return Err(invalid("a call is never proxied to this host itself"));
        }
        let doc = self.identity.document_now(did).await;
        let doc = doc.map_err(|_| invalid("could not resolve proxy did"))?;
        let found = did::service(&doc, &format!("#{service}"));
        let url = found.ok_or_else(|| invalid("could not resolve proxy did service url"))?;
        if url == self.config.public_url {
            return Err(invalid("a call is never proxied to this host itself"));
        }
        Ok(url)
    }

    /// Service auth from `group` for `lxm` at `aud`, lapsing at `exp`
    /// (seconds since 1970).
    fn service_jwt(&self, group: &str, aud: &str, lxm: Option<&str>, exp: i64) -> String {
        let account = self.store.account(group).ok().flatten();
        let account = account.expect("a group with a grant is here");
        let claims = Claims {
            iss: group.into(),
            aud: aud.into(),
            exp: exp as u64,
            iat: Some((now_millis() / 1000) as u64),
            lxm: lxm.map(String::from),
            jti: Some(jose::random(16)),
        };
        service_auth::sign(&account.signing, &claims)
    }
}

/// A method the host doesn't answer, sent on where `atproto-proxy` says,
/// as the group; without the header, refused as not implemented.
pub async fn proxy(
    State(host): State<Arc<Host>>,
    Path(nsid): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Refusal> {
    let Some(to) = headers.get("atproto-proxy") else {
        return Err(XrpcError::not_implemented(&nsid).into());
    };
    let to = to
        .to_str()
        .map_err(|_| invalid("invalid proxy header format"))?;
    if protected(&nsid) {
        return Err(bad_method().into());
    }
    let acting = host.acting(&headers, method.as_str(), &format!("/xrpc/{nsid}"))?;
    let (did, service) = proxy_target(to)?;
    host.may_call(&acting, &nsid, to, bad_method)?;
    let endpoint = host.endpoint(did, service).await?;
    let exp = now_millis() / 1000 + MINUTE;
    let token = host.service_jwt(&acting.grant.group, did, Some(&nsid), exp);
    let input = match method == Method::POST {
        true => Some(read_body(body, host.config.blob_upload_limit, "PayloadTooLarge").await?),
        false => None,
    };
    let path = uri.path_and_query().map_or(uri.path(), |p| p.as_str());
    let asked = host.guarded.request(method, &format!("{endpoint}{path}"));
    let mut sent = asked.map_err(|e| upstream(e.0))?.timeout(PROXIED_FOR);
    let encoding = headers.get("accept-encoding").cloned();
    sent = sent.header(
        "accept-encoding",
        encoding.unwrap_or("identity".parse().expect("a header")),
    );
    let content = ["content-type", "content-encoding"]
        .into_iter()
        .filter(|_| input.is_some());
    for name in SENT_ON.into_iter().chain(content) {
        if let Some(v) = headers.get(name) {
            sent = sent.header(name, v);
        }
    }
    for (name, v) in headers
        .iter()
        .filter(|(n, _)| n.as_str().starts_with("x-atproto-"))
    {
        sent = sent.header(name, v);
    }
    if let Some(input) = input {
        sent = sent.body(input);
    }
    let answered = sent.bearer_auth(token).send().await;
    let res = answered.map_err(|_| upstream("Upstream service unreachable"))?;
    let status = res.status().as_u16();
    tracing::info!(method = nsid, service = to, status, "proxied");
    Ok(answer(res).await)
}

/// A service's answer, as the caller is given it: a success streamed
/// with its content's headers, a failure as XRPC's, its name and message
/// the service's.
async fn answer(res: reqwest::Response) -> Response {
    let status = res.status();
    let mut out = Response::builder().status(status);
    let succeeded = status.as_u16() < 400;
    let names = PASSED_BACK
        .iter()
        .chain(CONTENT.iter().filter(|_| succeeded));
    for name in names {
        if let Some(v) = res.headers().get(*name) {
            out = out.header(*name, v);
        }
    }
    if succeeded {
        let chunks = futures::stream::try_unfold(res, |mut r| async move {
            Ok::<_, reqwest::Error>(r.chunk().await?.map(|c| (c, r)))
        });
        return out.body(Body::from_stream(chunks)).expect("a response");
    }
    let said = crate::index_api::capped(res, FAILURE_AT_MOST).await;
    let said: Value = serde_json::from_slice(&said.unwrap_or_default()).unwrap_or_default();
    let code = status.as_u16();
    let named = NAMES
        .iter()
        .find(|(s, _)| *s == code)
        .map_or("Unknown", |(_, n)| n);
    let error = said["error"].as_str().unwrap_or(named);
    let message = said["message"].as_str().unwrap_or(error);
    let status = match code {
        500 => StatusCode::BAD_GATEWAY,
        _ => status,
    };
    let body = Json(json!({ "error": error, "message": message }));
    let mut res = body.into_response();
    *res.status_mut() = status;
    let built = out.body(Body::empty()).expect("a response");
    res.headers_mut().extend(built.headers().clone());
    res
}

/// A token the group signs for an app to call `aud` with, for `lxm` or
/// any method, lapsing at `exp`: within an hour, or a minute for any
/// method, and a minute from now unless asked.
pub async fn get_service_auth(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Json<Value>, Refusal> {
    let nsid = "com.atproto.server.getServiceAuth";
    let acting = host.acting(&headers, "GET", &format!("/xrpc/{nsid}"))?;
    let aud = param(&p, "aud")?;
    let lxm = p.get("lxm").map(String::as_str);
    let insufficient = || {
        let lxm = lxm.unwrap_or_default();
        invalid(format!(
            "insufficient access to request a service auth token for the following method: {lxm}"
        ))
    };
    host.may_call(&acting, lxm.unwrap_or("*"), aud, insufficient)?;
    let (bare, service) = aud
        .split_once('#')
        .map_or((aud, None), |(d, s)| (d, Some(s)));
    if !is_did(bare) || service.is_some_and(|s| s.is_empty() || s.contains('#')) {
        let said = "aud must be a valid atproto DID or did#serviceId reference";
        return Err(invalid(said).into());
    }
    let now = now_millis() / 1000;
    let exp = match p.get("exp") {
        Some(e) => e
            .parse::<i64>()
            .map_err(|_| invalid("exp is a number of seconds"))?,
        None => now + MINUTE,
    };
    let bad = |said: &str| XrpcError::new(StatusCode::BAD_REQUEST, "BadExpiration", said);
    if exp < now {
        return Err(bad("expiration is in past").into());
    }
    if exp - now > HOUR {
        let said = "cannot request a token with an expiration more than an hour in the future";
        return Err(bad(said).into());
    }
    if lxm.is_none() && exp - now > MINUTE {
        let said = "cannot request a method-less token with an expiration more than a minute in the future";
        return Err(bad(said).into());
    }
    if let Some(lxm) = lxm.filter(|l| protected(l)) {
        let said = format!(
            "cannot request a service auth token for the following protected method: {lxm}"
        );
        return Err(invalid(said).into());
    }
    let token = host.service_jwt(&acting.grant.group, aud, lxm, exp);
    tracing::info!(aud, lxm, "gave service auth");
    Ok(Json(json!({ "token": token })))
}
