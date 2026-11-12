//! A group's website: the static files its `fyi.opensocial.site` record,
//! in its public repo, names, served as they are at its address,
//! `https://<address>/`: the name under the host's domain it was made at,
//! which stays its site's when it takes a handle on a domain of its own.
//! The host keeps `/.well-known/` there for itself.
//!
//! A group's address is a name under the host's own domain, so a browser
//! takes its pages for the same site as the console. Every answer here is
//! sandboxed (`Content-Security-Policy: sandbox`), which gives each page an
//! origin of its own that no other shares: it can neither read nor set a
//! cookie, and what it sends the host goes as from another site, without
//! the console's session. It may still run scripts, and ask the host's
//! public methods, which answer any origin.
//!
//! `putSite` publishes a site whole from the files given, as blobs of the
//! group's, a file it holds already used again; `deleteSite` takes it
//! down. Both require `group.configure`, as writing the record as the
//! group with a token does.

use crate::Host;
use crate::api::invalid;
use crate::group_api::{group_of, signed_in};
use crate::rbac::NS;
use crate::repo::{Asked, Write, record_cid};
use crate::site_head;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use atproto::drisl;
use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

/// The record of a group's site, at `self` in its public repo.
pub const SITE: &str = "fyi.opensocial.site";

/// How long a browser keeps a file before asking again.
const CACHE: &str = "public, max-age=60";

/// What every answer for a site is sandboxed by: scripts and forms run,
/// and links open, but in an origin of the page's own.
pub const SANDBOX: &str =
    "sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox";

/// A site the host would not serve, as `putSite` and a write of the
/// record refuse it.
fn refused(message: String) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidSite", message)
}

/// Whether `path` is one a site's file may be at: `/`-separated, with no
/// leading `/`, and no empty, `.` or `..` segment, nor a backslash or a
/// control character.
pub fn is_path(path: &str) -> bool {
    let fine = |s: &str| !s.is_empty() && s != "." && s != "..";
    let plain = !path.chars().any(|c| c == '\\' || c.is_control());
    plain && path.len() <= 512 && path.split('/').all(fine)
}

/// The blob a site's `file` names, if it names one.
fn blob_of(file: &Value) -> Option<atproto::Cid> {
    let blob = &file["blob"];
    let link = blob["ref"]["$link"]
        .as_str()
        .filter(|_| blob["$type"] == "blob")?;
    atproto::Cid::parse(link).ok()
}

/// Refuses a site the host would not serve: one whose paths [`paths`]
/// refuses, a file naming no blob, or a read that is none of [`READS`].
pub fn check(site: &Value) -> Result<(), XrpcError> {
    paths(site)?;
    for file in site["files"].as_array().into_iter().flatten() {
        if blob_of(file).is_none() {
            let path = file["path"].as_str().unwrap_or_default();
            return Err(refused(format!("{path} names no blob")));
        }
    }
    reads(site)
}

/// The host's methods a site's pages may ask, as whoever is looking and
/// signed in, through the page the host serves around them: reads alone.
pub const READS: &[&str] = &[
    "fyi.opensocial.getGroup",
    "fyi.opensocial.listMembers",
    "fyi.opensocial.listSpaceRecords",
    "fyi.opensocial.getSpaceBlob",
    "fyi.opensocial.getActor",
    "fyi.opensocial.getViewer",
];

/// The methods a site's `reads` name.
fn reads_of(site: &Value) -> Vec<&str> {
    let all = site["reads"].as_array().into_iter().flatten();
    all.filter_map(|r| r["method"].as_str()).collect()
}

/// Refuses a site whose `reads` name a method that is none of [`READS`].
fn reads(site: &Value) -> Result<(), XrpcError> {
    match reads_of(site).into_iter().find(|m| !READS.contains(m)) {
        Some(method) => Err(refused(format!(
            "{method} is none of the methods a site's pages may ask"
        ))),
        None => Ok(()),
    }
}

/// Refuses a site's paths: a file at a path no file may be at, or at one
/// another has; a `fallback` or `notFound` that is none of its files.
pub fn paths(site: &Value) -> Result<(), XrpcError> {
    let mut paths = BTreeSet::new();
    for file in site["files"].as_array().into_iter().flatten() {
        let path = file["path"].as_str().unwrap_or_default();
        if !is_path(path) {
            return Err(refused(format!(
                "{path:?} is not a path a site's file may be at"
            )));
        }
        if !paths.insert(path) {
            return Err(refused(format!("{path} is given more than once")));
        }
    }
    for key in ["fallback", "notFound"] {
        let named = site[key].as_str();
        if let Some(path) = named.filter(|p| !paths.contains(p)) {
            return Err(refused(format!(
                "{key} is {path}, which is none of the files"
            )));
        }
    }
    Ok(())
}

/// The type a file at `path` is served as, by its extension, when its
/// record names none.
pub fn content_type(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or_default();
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match extension.as_deref().unwrap_or_default() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        "wasm" => "application/wasm",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// `path` with each `%XX` read as the byte it names, if it is UTF-8.
pub fn decoded(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).filter(|_| bytes[i] == b'%');
        let byte = hex.and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match byte {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// `path` with every byte but a letter, a digit, `-._~` and `/` written
/// `%XX`, to stand in a `Location` as the path it is.
pub fn encoded(path: &str) -> String {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"-._~/".contains(&b);
    let each = path.bytes().map(|b| match plain(b) {
        true => (b as char).to_string(),
        false => format!("%{b:02X}"),
    });
    each.collect()
}

/// What a request for a site's page asks of it.
struct Asking {
    path: String,
    /// Whether a page, as a single-page app's routes are: one asking for
    /// HTML, or a path whose last segment has no extension.
    page: bool,
    /// The ETag the browser holds.
    held: Option<String>,
}

/// An answer for a site, sandboxed.
fn sandboxed(mut res: Response) -> Response {
    let headers = res.headers_mut();
    let set = [
        (header::CONTENT_SECURITY_POLICY, SANDBOX),
        (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
    ];
    for (name, value) in set {
        headers.insert(name, HeaderValue::from_static(value));
    }
    res
}

fn plain(status: StatusCode, text: &'static str) -> Response {
    let kind = [(header::CONTENT_TYPE, "text/plain; charset=utf-8")];
    (status, kind, text).into_response()
}

fn moved(to: &str, status: StatusCode) -> Response {
    let to = HeaderValue::from_str(to).expect("a path or address the host made");
    (status, [(header::LOCATION, to)]).into_response()
}

impl Host {
    /// The group whose address a request to `name` names, and its handle,
    /// unless `name` is the host's own.
    fn site_of(&self, name: &str) -> Option<(String, String)> {
        let name = name
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let own = self
            .config
            .public_url
            .split("://")
            .nth(1)
            .unwrap_or_default();
        if name.is_empty() || name == own.split(':').next().unwrap_or_default() {
            return None;
        }
        let did = self.store.addressed(&name).ok().flatten()?;
        let account = self.store.account(&did).ok().flatten()?;
        Some((account.did, account.handle))
    }

    /// The file of `did`'s site at `path`: its type and bytes, and its ETag.
    fn site_file(
        &self,
        did: &str,
        site: &Value,
        path: &str,
    ) -> Result<Option<(String, Vec<u8>, String)>, XrpcError> {
        let files = site["files"].as_array().into_iter().flatten();
        // A record copied from another host was never checked here: a file
        // naming no blob is as none.
        let found = files.into_iter().find(|f| f["path"] == path);
        let Some((file, cid)) = found.and_then(|f| Some((f, blob_of(f)?))) else {
            return Ok(None);
        };
        let read = self.read_blob(did, &cid).map_err(XrpcError::internal)?;
        let kind = file["contentType"].as_str().unwrap_or(content_type(path));
        let etag = format!("\"{cid}\"");
        Ok(read.map(|(_, bytes)| (kind.to_string(), bytes, etag)))
    }

    /// The answer of `did`'s site, `handle`, to `asking`, its own folders
    /// redirected under `under`.
    fn site_answer(
        &self,
        did: &str,
        handle: &str,
        asking: &Asking,
        under: &str,
    ) -> Result<Response, XrpcError> {
        if self.active(did).is_err() {
            return Ok(plain(StatusCode::NOT_FOUND, "no site here"));
        }
        let record = self.store.record(did, SITE, "self");
        let Some(record) = record.map_err(XrpcError::internal)? else {
            if asking.path.is_empty() {
                let page = format!("{}/g/{handle}", self.config.console_url);
                return Ok(moved(&page, StatusCode::FOUND));
            }
            return Ok(plain(StatusCode::NOT_FOUND, "no site here"));
        };
        let site = drisl::decode(&record.value)
            .expect("a record the host wrote")
            .to_json();
        let path = &asking.path;
        let index = format!("{path}index.html");
        let wanted = match path.is_empty() || path.ends_with('/') {
            true => &index,
            false => path,
        };
        if let Some(found) = self.site_file(did, &site, wanted)? {
            return Ok(file(StatusCode::OK, found, asking));
        }
        if self
            .site_file(did, &site, &format!("{path}/index.html"))?
            .is_some()
        {
            return Ok(moved(
                &format!("{under}/{}/", encoded(path)),
                StatusCode::MOVED_PERMANENTLY,
            ));
        }
        let fallback = site["fallback"].as_str().filter(|_| asking.page);
        let missing = [
            (fallback, StatusCode::OK),
            (site["notFound"].as_str(), StatusCode::NOT_FOUND),
        ];
        for (named, status) in missing {
            if let Some(found) = named
                .map(|p| self.site_file(did, &site, p))
                .transpose()?
                .flatten()
            {
                return Ok(file(status, found, asking));
            }
        }
        Ok(plain(StatusCode::NOT_FOUND, "no such page"))
    }

    /// `did`'s site as its record has it, while the group is on.
    fn live_site(&self, did: &str) -> Option<Value> {
        self.active(did).ok()?;
        let record = self.store.record(did, SITE, "self").ok().flatten()?;
        let site = drisl::decode(&record.value).expect("a record the host wrote");
        Some(site.to_json())
    }

    /// The page of `did`'s `site` the frame shows at `path`, and the status
    /// it is served with: the file there or the folder's `index.html`, else
    /// its `fallback`, else, not found, its `notFound`. Only HTML is read.
    fn page_at(&self, did: &str, site: &Value, path: &str) -> (StatusCode, Option<String>) {
        let tried = match path.is_empty() || path.ends_with('/') {
            true => vec![format!("{path}index.html")],
            false => vec![path.to_string(), format!("{path}/index.html")],
        };
        let fallback = site["fallback"].as_str().map(String::from);
        let found = |paths: Vec<String>| {
            let file = paths
                .iter()
                .find_map(|p| self.site_file(did, site, p).ok().flatten());
            let html = file.filter(|(kind, ..)| kind.starts_with("text/html"));
            html.and_then(|(_, bytes, _)| String::from_utf8(bytes).ok())
        };
        let shown = tried.into_iter().chain(fallback).collect::<Vec<_>>();
        if let Some(html) = found(shown) {
            return (StatusCode::OK, Some(html));
        }
        let missing = site["notFound"].as_str().map(String::from);
        (StatusCode::NOT_FOUND, found(missing.into_iter().collect()))
    }
}

/// A file of a site's, or that the browser holds it already.
fn file(
    status: StatusCode,
    (kind, bytes, etag): (String, Vec<u8>, String),
    asking: &Asking,
) -> Response {
    let kind = HeaderValue::from_str(&kind)
        .unwrap_or(HeaderValue::from_static("application/octet-stream"));
    let tag = HeaderValue::from_str(&etag).expect("a CID in quotes");
    let headers = [
        (header::CACHE_CONTROL, HeaderValue::from_static(CACHE)),
        (header::ETAG, tag),
    ];
    if status == StatusCode::OK && asking.held.as_deref() == Some(etag.as_str()) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }
    (status, headers, [(header::CONTENT_TYPE, kind)], bytes).into_response()
}

/// Answers a request to a group's handle from its site, but for the
/// host's own paths under `/.well-known/`; passes on any other.
pub async fn serve(State(host): State<Arc<Host>>, req: Request, next: Next) -> Response {
    let named = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok());
    let group = named.and_then(|n| host.site_of(n));
    // The host's own paths there: its `/.well-known/`, and its methods,
    // which the page around a site asks (a handle being typed, say).
    let path = req.uri().path();
    let ours = path.starts_with("/.well-known/") || path.starts_with("/xrpc/");
    let (Some((did, handle)), false) = (group, ours) else {
        return next.run(req).await;
    };
    if ![Method::GET, Method::HEAD].contains(req.method()) {
        let mut res = plain(StatusCode::METHOD_NOT_ALLOWED, "a site is only read");
        res.headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
        return sandboxed(res);
    }
    let Some(path) = decoded(req.uri().path().trim_start_matches('/')) else {
        return sandboxed(plain(StatusCode::NOT_FOUND, "no such page"));
    };
    let headers = req.headers();
    let text = |name| {
        headers
            .get(name)
            .and_then(|v: &HeaderValue| v.to_str().ok())
    };
    let html = text(header::ACCEPT).is_some_and(|a| a.contains("text/html"));
    let last = path.rsplit('/').next().unwrap_or_default();
    let asking = Asking {
        page: html || !last.contains('.'),
        held: text(header::IF_NONE_MATCH).map(String::from),
        path,
    };
    // The name the browser asked, as a proxy in front says it if it says,
    // else as the request does: the site's OAuth client is named for it.
    let asked = text(HeaderName::from_static("x-forwarded-host"));
    match host.live_site(&did).filter(|s| !reads_of(s).is_empty()) {
        Some(site) => {
            let origin = format!("https://{}", asked.or(named).unwrap_or_default());
            live(&host, &did, &handle, &site, &origin, asking)
        }
        None => sandboxed(
            host.site_answer(&did, &handle, &asking, "")
                .unwrap_or_else(IntoResponse::into_response),
        ),
    }
}

/// The script a site's page runs to ask the page around it, `os`.
const KIT: &str = include_str!("site_kit.js");

/// What the page around a site's pages allows itself: its own script, and
/// calls to any PDS its visitor signs in at.
const SHELL_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
     connect-src *; img-src * data:; frame-src 'self'; frame-ancestors 'none'; base-uri 'none'; \
     form-action 'none'";

/// The answer of a site that asks for live data, as the browser at
/// `origin` asked it: its files under `/_site/`, sandboxed; its OAuth
/// client document, named for `origin`; the `os` script and the page's
/// own; and at every other page, the page the host serves around them,
/// holding the visitor's session.
fn live(
    host: &Host,
    did: &str,
    handle: &str,
    site: &Value,
    origin: &str,
    mut asking: Asking,
) -> Response {
    let reads = reads_of(site);
    match asking.path.as_str() {
        "client-metadata.json" => client_metadata(host, handle, origin, &reads),
        "_os/kit.js" => script(KIT.as_bytes().to_vec()),
        "_os/shell.js" => {
            let built = host.config.site_shell.as_ref().map(|d| d.join("shell.js"));
            match built.map(std::fs::read) {
                Some(Ok(bytes)) => script(bytes),
                _ => plain(StatusCode::NOT_FOUND, "no page to run a site in here"),
            }
        }
        path if path.starts_with("_site/") => {
            asking.path = path["_site/".len()..].to_string();
            let answer = host.site_answer(did, handle, &asking, "/_site");
            sandboxed(answer.unwrap_or_else(IntoResponse::into_response))
        }
        _ if asking.page => shell(host, did, handle, site, &asking.path),
        _ => {
            let answer = host.site_answer(did, handle, &asking, "");
            sandboxed(answer.unwrap_or_else(IntoResponse::into_response))
        }
    }
}

fn script(bytes: Vec<u8>) -> Response {
    let kind = [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")];
    (StatusCode::OK, kind, bytes).into_response()
}

/// The site's OAuth client document, at its address `origin`: `atproto`,
/// and leave to ask each of `reads`, and `getViewer`, of the host.
fn client_metadata(host: &Host, handle: &str, origin: &str, reads: &[&str]) -> Response {
    let aud = format!("{}#opensocial", host.config.service_did);
    let aud = reqwest::Url::parse_with_params("https://x/", &[("aud", aud)])
        .expect("a URL")
        .query()
        .unwrap_or_default()
        .to_string();
    let mut scope = vec!["atproto".to_string()];
    let viewer = "fyi.opensocial.getViewer";
    for method in reads.iter().chain([&viewer]).collect::<BTreeSet<_>>() {
        scope.push(format!("rpc:{method}?{aud}"));
    }
    let doc = json!({
        "client_id": format!("{origin}/client-metadata.json"),
        "client_name": handle,
        "client_uri": origin,
        "redirect_uris": [format!("{origin}/_os/callback")],
        "scope": scope.join(" "),
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "application_type": "web",
        "token_endpoint_auth_method": "none",
        "dpop_bound_access_tokens": true,
    });
    let mut res = Json(doc).into_response();
    res.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    res
}

/// The page around a site's pages at `path`: the group's page there in a
/// sandboxed frame, and what the host's script needs to sign its visitor
/// in and ask `reads` as them.
fn shell(host: &Host, did: &str, handle: &str, site: &Value, path: &str) -> Response {
    let reads = reads_of(site);
    let (status, html) = host.page_at(did, site, path);
    let head = site_head::preview(html.as_deref().unwrap_or_default(), handle);
    let config = json!({
        "group": did,
        "handle": handle,
        "reads": reads,
        "aud": format!("{}#opensocial", host.config.service_did),
        "handleResolver": host.config.site_handle_resolver,
        "plc": host.config.plc_url,
        "signUpAt": host.config.signup_pds,
        "console": host.config.console_url,
        "allowHttp": host.config.public_url.starts_with("http://"),
    });
    // In a script element, `<` is the one thing that could end it early.
    let config = config.to_string().replace('<', "\\u003c");
    let frame = format!("/_site/{}", encoded(path));
    let page = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         {head}</head><body>\
         <iframe id=\"os-page\" title=\"{handle}\" data-src=\"{frame}\" \
         sandbox=\"allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox\"></iframe>\
         <script type=\"application/json\" id=\"os-config\">{config}</script>\
         <script type=\"module\" src=\"/_os/shell.js\"></script></body></html>"
    );
    let headers = [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (header::CONTENT_SECURITY_POLICY, SHELL_POLICY),
        (header::CACHE_CONTROL, "no-cache"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ];
    (status, headers, page).into_response()
}

type Answer = Result<Json<Value>, XrpcError>;

/// The group a body names, once its caller of `lxm` may configure it.
async fn configuring(
    host: &Host,
    headers: &HeaderMap,
    body: &Value,
    lxm: &str,
) -> Result<String, XrpcError> {
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(host, group)?;
    if !host.may_configure(group, &me)? {
        return Err(crate::group_config::forbidden());
    }
    Ok(group.to_string())
}

/// A file given to `putSite`, as the site's record names it, kept as a
/// blob of `group`'s: whether it was new to the group.
fn kept(host: &Host, group: &str, given: &Value) -> Result<(Value, bool), XrpcError> {
    let path = given["path"].as_str().expect("its lexicon requires it");
    // Its lexicon has checked bytes given are base64.
    let bytes = match (&given["text"], &given["bytes"]) {
        (Value::String(text), Value::Null) => Some(text.as_bytes().to_vec()),
        (Value::Null, b @ Value::Object(_)) => drisl::Value::from_json(b)
            .ok()
            .and_then(drisl::Value::into_bytes),
        _ => None,
    };
    let one = || refused(format!("{path} gives its text or its bytes, one of them"));
    let bytes = bytes.ok_or_else(one)?;
    let kind = given["contentType"].as_str().unwrap_or(content_type(path));
    let cid = atproto::Cid::of_raw(&bytes);
    let held = host.store.blob(group, &cid).map_err(XrpcError::internal)?;
    let held = held.filter(|b| !b.temporary);
    let new = held.is_none();
    let blob = match held {
        Some(b) => {
            json!({"$type": "blob", "ref": {"$link": cid.to_string()}, "mimeType": b.mime_type, "size": b.size})
        }
        None => host.upload_blob(group, &bytes, kind.split(';').next().unwrap_or(kind))?,
    };
    let mut file = json!({"path": path, "blob": blob});
    if let Some(kind) = given.get("contentType") {
        file["contentType"] = kind.clone();
    }
    Ok((file, new))
}

pub async fn put_site(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = configuring(&host, &headers, &body, "putSite").await?;
    host.active(&group)?;
    let mut site = json!({"$type": SITE, "files": [], "createdAt": iso(now_millis())});
    for key in ["fallback", "notFound", "reads"] {
        if let Some(given) = body.get(key) {
            site[key] = given.clone();
        }
    }
    reads(&site)?;
    let given = body["files"].as_array().expect("its lexicon requires it");
    let names: Vec<Value> = given.iter().map(|f| json!({"path": f["path"]})).collect();
    paths(&json!({"files": names, "fallback": site["fallback"], "notFound": site["notFound"]}))?;
    let mut uploaded = 0;
    for file in given {
        let (file, new) = kept(&host, &group, file)?;
        uploaded += usize::from(new);
        site["files"]
            .as_array_mut()
            .expect("made a list")
            .push(file);
    }
    let write = Write::Update {
        collection: SITE.into(),
        rkey: "self".into(),
        value: site.clone(),
    };
    let asked = Asked {
        put: true,
        ..Asked::default()
    };
    host.write_asked(&group, std::slice::from_ref(&write), &asked)?;
    let address = host.store.address(&group).map_err(XrpcError::internal)?;
    let address = address.expect("a group has an address");
    let scheme = host
        .config
        .public_url
        .split("://")
        .next()
        .unwrap_or("https");
    Ok(Json(json!({
        "uri": format!("at://{group}/{SITE}/self"),
        "cid": record_cid(SITE, &site)?.to_string(),
        "url": format!("{scheme}://{address}/"),
        "uploaded": uploaded,
    })))
}

pub async fn delete_site(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = configuring(&host, &headers, &body, "deleteSite").await?;
    let write = Write::Delete {
        collection: SITE.into(),
        rkey: "self".into(),
    };
    let asked = Asked {
        put: true,
        ..Asked::default()
    };
    host.write_asked(&group, std::slice::from_ref(&write), &asked)?;
    Ok(Json(json!({})))
}

impl Host {
    /// Refuses a write of the group's site, in its public repo, by a
    /// token whose member's `roles` under `grants` do not carry
    /// `group.configure`, or of a site the host would not serve.
    pub(crate) fn site_write(&self, configures: bool, write: &Write) -> Result<(), XrpcError> {
        let (collection, rkey) = write.path();
        if collection != SITE {
            return Ok(());
        }
        if rkey != "self" {
            return Err(invalid(format!("{collection} is kept at self")));
        }
        if !configures {
            return Err(crate::group_config::forbidden());
        }
        match write {
            Write::Create { value, .. } | Write::Update { value, .. } => check(value),
            Write::Delete { .. } => Ok(()),
        }
    }
}
