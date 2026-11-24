//! The space index, served: what is in one of a group's spaces, and the
//! pictures and files there, for whoever its policy lets read the
//! space, signed in, asked on every call. The index holds copies; it
//! never decides who sees them. What the group takes down with a
//! `!takedown` label (`takedown.rs`) it serves to nobody.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::authority::Access::Write;
use crate::group_api::{group_of, signed_in};
use crate::rbac::NS;
use crate::store::Indexed;
use crate::time::iso;
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use atproto::space_auth::{self, Kind, Make, sign_request};
use atproto::{Cid, Curve, Keypair, did, drisl};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::sync::Arc;

/// How long a list waits on the index to read the space's writers.
const SYNC_AT_MOST: std::time::Duration = std::time::Duration::from_secs(5);

/// The most bytes a file is fetched from its writer's PDS for.
pub const FILE_AT_MOST: usize = 5 * 1024 * 1024;

fn not_found(message: &str) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "NotFound", message)
}

/// The space a read names, once it is one of the group's and the caller
/// may read it by its policy.
async fn readable(
    host: &Host,
    p: &Params,
    headers: &HeaderMap,
    lxm: &str,
) -> Result<(String, String), XrpcError> {
    let group = param(p, "group")?;
    let space = param(p, "space")?;
    group_of(host, group).map_err(|_| not_found("unknown group"))?;
    host.active(group).map_err(|_| not_found("unknown group"))?;
    // Someone not signed in reads no space, a public one included.
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    let named = SpaceRef::parse(space).ok().filter(|s| s.authority == group);
    let policy = match named {
        Some(_) => host.policy(space).map_err(XrpcError::internal)?,
        None => None,
    };
    let policy = policy.ok_or_else(|| not_found("not one of this group's spaces"))?;
    let roles = host.roles(group, &me).map_err(XrpcError::internal)?;
    if !policy.can_read(&roles) {
        return Err(XrpcError::new(
            StatusCode::UNAUTHORIZED,
            "NotAuthorized",
            "not readable by you",
        ));
    }
    Ok((group.to_string(), space.to_string()))
}

/// The value at a dotted path in a record, as text.
fn at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(value, |v, key| v.get(key))
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Records in one of a group's spaces, from the index: one collection or
/// every one, newest first, a page at a time; only those whose `field`
/// equals `equals`, if it says; or the one at `uri`. None the group has
/// taken down (`takedown.rs`).
pub async fn list_space_records(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Json<Value>, XrpcError> {
    let (group, space) = readable(&host, &p, &headers, "listSpaceRecords").await?;
    // A space with writes the index has not read yet is read first.
    // Read at most a few seconds: past that, the index as it is.
    let synced = tokio::time::timeout(SYNC_AT_MOST, host.sync_space(&space)).await;
    tracing::debug!(space, ?synced, "read before listing");
    let limit = page(&p, 50, 100)?;
    let down = host
        .taken_down(&group, &space)
        .map_err(XrpcError::internal)?;
    let uri_of = |r: &Indexed| format!("{space}/{}/{}/{}", r.did, r.collection, r.rkey);
    let hidden = |r: &Indexed| down.hides(&uri_of(r), &r.did, &r.cid.to_string());
    let view = |r: &Indexed| {
        let value = drisl::decode(&r.value)
            .expect("a record the host indexed")
            .to_json();
        json!({
            "uri": uri_of(r),
            "cid": r.cid.to_string(),
            "repo": r.did,
            "value": value,
            "createdAt": iso(r.indexed_at),
        })
    };
    if let Some(uri) = p.get("uri") {
        let rest = uri.strip_prefix(&format!("{space}/"));
        let parts: Vec<&str> = rest.unwrap_or_default().splitn(3, '/').collect();
        let [did, collection, rkey] = parts[..] else {
            return Err(not_found("not a record in this space"));
        };
        let one = host
            .store
            .indexed_one(&space, did, &format!("{collection}/{rkey}"))
            .map_err(XrpcError::internal)?;
        let records: Vec<Value> = one.iter().filter(|r| !hidden(r)).map(view).collect();
        return Ok(Json(json!({ "records": records })));
    }
    let collection = p.get("collection").map(String::as_str);
    let mut before = match p.get("cursor") {
        None => None,
        Some(c) => Some(
            c.parse::<i64>()
                .map_err(|_| invalid("a cursor this host gave"))?,
        ),
    };
    let wanted = p
        .get("field")
        .map(|f| (f.as_str(), p.get("equals").map_or("", String::as_str)));
    let mut records = Vec::new();
    let mut cursor = None;
    // A page of what matches: read on through the index until it is full.
    loop {
        let batch = host
            .store
            .indexed(&space, collection, before, limit)
            .map_err(XrpcError::internal)?;
        let ended = batch.len() < limit as usize;
        for r in &batch {
            before = Some(r.seq);
            if hidden(r) {
                continue;
            }
            let shown = view(r);
            let matches = wanted.is_none_or(|(field, equals)| {
                at_path(&shown["value"], field).is_some_and(|v| text_of(v) == equals)
            });
            if matches {
                records.push(shown);
                if records.len() == limit as usize {
                    cursor = Some(r.seq.to_string());
                    break;
                }
            }
        }
        if cursor.is_some() || ended {
            break;
        }
    }
    Ok(Json(json!({"records": records, "cursor": cursor})))
}

/// A picture or file in one of the group's spaces, read from whoever wrote
/// it, as the space's authority: a page never reaches anyone's own
/// server for it. Served as a download.
pub async fn get_space_blob(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, XrpcError> {
    let (group, space) = readable(&host, &p, &headers, "getSpaceBlob").await?;
    let repo = param(&p, "repo")?;
    let cid = Cid::parse(param(&p, "cid")?).map_err(|e| invalid(format!("cid: {e}")))?;
    // One the group has taken down is not served, as though not there.
    let down = host
        .taken_down(&group, &space)
        .map_err(XrpcError::internal)?;
    let gone = host.file_taken_down(&down, &space, repo, &cid);
    if gone.map_err(XrpcError::internal)? {
        return Err(not_found("no such file in this space"));
    }
    let fetched = host.space_blob(&group, &space, repo, &cid).await;
    let (mime, bytes) = fetched.ok_or_else(|| not_found("no such file in this space"))?;
    let headers = [
        (header::CONTENT_TYPE, mime),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox".to_string(),
        ),
        (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
    ];
    Ok((headers, bytes).into_response())
}

impl Host {
    /// `repo`'s blob `cid` in `space`, a group's here: from the store for
    /// a repo here, else from the repo's PDS, with a credential the group
    /// issues itself. Its type and bytes.
    async fn space_blob(
        &self,
        group: &str,
        space: &str,
        repo: &str,
        cid: &Cid,
    ) -> Option<(String, Vec<u8>)> {
        // Only a writer of the space, and one it still lets write, as the
        // index keeps: any other DID would have the host ask a server of
        // the caller's choosing, with the group's credential, and one put
        // out has what they wrote there forgotten.
        let writers = self.store.writers(space, None, u32::MAX).ok()?;
        let hosted = self.store.space(space).ok()??;
        let writes = self.may(space, &hosted, Some(repo), Write).ok()?;
        if !writes || !writers.iter().any(|w| w.did == repo) {
            return None;
        }
        let here = self
            .store
            .account(repo)
            .ok()?
            .is_some_and(|a| a.did == repo);
        if here {
            if !self.store.space_has_blob(repo, space, cid).ok()? {
                return None;
            }
            let (blob, bytes) = self.read_blob(repo, cid).ok()??;
            return Some((blob.mime_type, bytes));
        }
        let doc = self.identity.document(repo).await.ok()?;
        let pds = did::pds(&doc)?;
        let key = Keypair::generate(Curve::P256);
        let signing = self.store.account(group).ok()??.signing;
        let kid = key.public().did_key();
        let make = Make {
            iss: group,
            sub: space,
            key_id: Some(&kid),
            ..Make::default()
        };
        let now = crate::time::now_millis() as u64 / 1000;
        let credential = space_auth::create(Kind::Credential, &make, &signing, now).ok()?;
        let authorization = format!("Atproto-Space-Credential {credential}");
        let mut req = self
            .http
            .get(format!("{pds}/xrpc/com.atproto.space.getBlob"))
            .query(&[("space", space), ("repo", repo), ("cid", &cid.to_string())]);
        for (k, v) in sign_request(&key, &authorization, repo).ok()? {
            req = req.header(k, v);
        }
        let res = req.send().await.ok()?.error_for_status().ok()?;
        let mime = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());
        let mime = mime.unwrap_or("application/octet-stream").to_string();
        let bytes = capped(res, FILE_AT_MOST).await?;
        // What the writer sent must be the file asked for.
        (Cid::of_raw(&bytes) == *cid).then_some((mime, bytes))
    }
}

/// A response's bytes, if they are no more than `max`.
pub(crate) async fn capped(mut res: reqwest::Response, max: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.ok()? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > max {
            return None;
        }
    }
    Some(bytes)
}
