//! A group moved from one group host to another.
//!
//! The group's identity moves by a PLC operation the source signs with its
//! rotation key, and everything the group holds moves with it: its public
//! repo, its files, and in each of its spaces its own repo and what the
//! space's authority keeps (who writes there, and what they last wrote).
//! The destination pulls all of it from the source, checks it against the
//! group's key and against itself, before the directory changes, and keeps
//! the group switched off until it does. Members' repos in the group's
//! spaces are theirs, at their own PDSes, and stay there: the destination
//! keeps who writes, so it reads them on without waiting for them to write
//! again, and their PDSes find the new authority by the group's DID.
//!
//! The steps, each a method, which the `opensocial` command walks through:
//!
//! 1. `migratePrepare`, at the source: a service token the group signs, for
//!    the destination; the group's handle and spaces; and a token, good for
//!    an hour, to pull the rest with (`migrateExport`).
//! 2. `migrateReceive`, at the destination: the group pulled and kept,
//!    switched off; what the PLC operation should name.
//! 3. `migrateComplete`, at the source: the operation signed and sent, and
//!    the group switched off there.
//! 4. `migrateActivate`, at the destination: once the directory names it,
//!    the group switched on.
//!
//! A move is between two of these hosts, so a space's state moves as
//! this host keeps it, not as the space alpha's meta archive.

use crate::Host;
use crate::api::{ANSWER_AT_MOST, Params, REPO_AT_MOST, invalid, param, too_much};
use crate::group_api::signed_in;
use crate::index_api::capped;
use crate::rbac::NS;
use crate::repo::{Asked, Write};
use crate::store::{Account, Store};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::service_auth::{self, Claims};
use atproto::space::{LtHash, SpaceRef, element, read_star, star};
use atproto::{Cid, Curve, Keypair, car, did, drisl, jose, mst::Tree};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// How long the destination may pull the group with its export token.
const EXPORT_FOR: i64 = 60 * 60_000;
/// The method the group's service token is for.
pub const RECEIVE: &str = "fyi.opensocial.migrateReceive";

fn refused(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// A JSON answer of the group's host, read up to `ANSWER_AT_MOST`.
async fn json_at_most(res: reqwest::Response) -> Result<Value, XrpcError> {
    let read = capped(res, ANSWER_AT_MOST).await;
    let bytes = read.ok_or(source(too_much(ANSWER_AT_MOST)))?;
    serde_json::from_slice(&bytes).map_err(source)
}

fn source(why: impl std::fmt::Display) -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_GATEWAY,
        "SourceUnreadable",
        format!("the group's host: {why}"),
    )
}

impl Store {
    /// Keeps `group`'s export token, by its hash, until `until`.
    pub fn allow_export(&self, group: &str, hash: &str, until: i64) -> Result<()> {
        let sql = "INSERT INTO migration_export (did, token_hash, expires_at) VALUES (?1, ?2, ?3)
                   ON CONFLICT (did) DO UPDATE SET token_hash = ?2, expires_at = ?3";
        self.conn().execute(sql, params![group, hash, until])?;
        Ok(())
    }

    /// `group`'s export token, ended.
    pub fn end_export(&self, group: &str) -> Result<()> {
        let sql = "DELETE FROM migration_export WHERE did = ?1";
        self.conn().execute(sql, [group])?;
        Ok(())
    }

    /// Whether `hash` is `group`'s export token, unexpired at `now`.
    pub fn may_export(&self, group: &str, hash: &str, now: i64) -> Result<bool> {
        let sql =
            "SELECT 1 FROM migration_export WHERE did = ?1 AND token_hash = ?2 AND expires_at > ?3";
        let found: Option<i64> = self
            .conn()
            .query_row(sql, params![group, hash, now], |r| r.get(0))
            .optional()?;
        Ok(found.is_some())
    }

    /// `did`'s files that records hold, and their types.
    pub fn held_blobs(&self, did: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn();
        let mut q = conn.prepare(
            "SELECT cid, mime_type FROM blob WHERE did = ?1 AND temporary = 0 ORDER BY cid",
        )?;
        let rows = q.query_map([did], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Who may read or write `space` beyond what its access record says.
    pub fn grants(&self, space: &str) -> Result<Vec<(String, bool, bool)>> {
        let conn = self.conn();
        let mut q =
            conn.prepare("SELECT did, read, write FROM space_grant WHERE space = ?1 ORDER BY did")?;
        let rows = q.query_map([space], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl Host {
    /// The group a request names, for `me`, who may configure it.
    async fn mover(
        &self,
        headers: &HeaderMap,
        lxm: &str,
        group: Option<&str>,
    ) -> Result<(String, String), XrpcError> {
        let me = signed_in(self, headers, &format!("{NS}.{lxm}")).await?;
        let group = group.expect("its lexicon requires it");
        crate::group_api::group_of(self, group)?;
        if !self.may_do(group, &me, "group.configure")? {
            return Err(XrpcError::new(
                StatusCode::FORBIDDEN,
                "Forbidden",
                "missing group.configure",
            ));
        }
        Ok((group.to_string(), me))
    }
}

/// Step 1, at the source: what the destination needs to take the group.
pub async fn migrate_prepare(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, _) = host
        .mover(&headers, "migratePrepare", body["group"].as_str())
        .await?;
    let dest = body["destDid"]
        .as_str()
        .filter(|d| atproto::syntax::is_did(d));
    let dest = dest.expect("its lexicon requires it");
    let last = host
        .plc
        .last_op(&group)
        .await
        .map_err(XrpcError::internal)?;
    let ours = host.secrets.rotation.public().did_key();
    let holds = matches!(&last, atproto::plc::Signed::Update { op, .. } if op.rotation_keys.contains(&ours));
    if !holds {
        return Err(refused(
            "NotController",
            "this host no longer holds a rotation key for the group",
        ));
    }
    let account = host
        .store
        .account(&group)
        .map_err(XrpcError::internal)?
        .expect("a group here");
    let now = now_millis();
    let token = jose::random(32);
    host.store
        .allow_export(&group, &jose::sha256(&token), now + EXPORT_FOR)
        .map_err(XrpcError::internal)?;
    let claims = Claims {
        iss: group.clone(),
        aud: dest.into(),
        exp: ((now + EXPORT_FOR) / 1000) as u64,
        iat: Some((now / 1000) as u64),
        lxm: Some(RECEIVE.into()),
        jti: Some(jose::random(12)),
    };
    let jwt = service_auth::sign(&account.signing, &claims);
    let spaces = host.store.spaces_of(&group).map_err(XrpcError::internal)?;
    let address = host.store.address(&group).map_err(XrpcError::internal)?;
    let snapshot = json!({"handle": account.handle, "address": address, "spaces": spaces});
    Ok(Json(
        json!({"serviceJwt": jwt, "exportToken": token, "snapshot": snapshot}),
    ))
}

/// At the source, for the destination: a part of the group nothing public
/// serves whole, with the export token.
pub async fn migrate_export(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
) -> Result<Response, XrpcError> {
    let group = param(&p, "group")?;
    let token = param(&p, "token")?;
    let allowed = host
        .store
        .may_export(group, &jose::sha256(token), now_millis());
    if !allowed.map_err(XrpcError::internal)? {
        return Err(XrpcError::new(
            StatusCode::UNAUTHORIZED,
            "InvalidToken",
            "not a token to export this group with",
        ));
    }
    match param(&p, "part")? {
        "blobs" => {
            let held = host.store.held_blobs(group).map_err(XrpcError::internal)?;
            let blobs: Vec<Value> = held
                .iter()
                .map(|(cid, mime)| json!({"cid": cid, "mimeType": mime}))
                .collect();
            Ok(Json(json!({ "blobs": blobs })).into_response())
        }
        "blob" => {
            let cid = Cid::parse(param(&p, "cid")?).map_err(|e| invalid(format!("cid: {e}")))?;
            let read = host.read_blob(group, &cid).map_err(XrpcError::internal)?;
            let (blob, bytes) =
                read.ok_or_else(|| refused("BlobNotFound", "no such file of the group's"))?;
            Ok(([(header::CONTENT_TYPE, blob.mime_type)], bytes).into_response())
        }
        part => {
            let space = param(&p, "space")?;
            let hosted = host.store.space(space).map_err(XrpcError::internal)?;
            let hosted = hosted.filter(|s| s.authority == group);
            let hosted = hosted.ok_or_else(|| invalid("not one of the group's spaces"))?;
            if part == "meta" {
                let writers = host
                    .store
                    .writers(space, None, u32::MAX)
                    .map_err(XrpcError::internal)?;
                let writers: Vec<Value> = writers
                    .iter()
                    .filter(|w| w.did != group)
                    .map(|w| json!({"did": w.did, "repoRev": w.repo_rev, "data": jose::b64(&w.data)}))
                    .collect();
                let grants = host.store.grants(space).map_err(XrpcError::internal)?;
                let grants: Vec<Value> = grants
                    .iter()
                    .map(|(d, r, w)| json!({"did": d, "read": r, "write": w}))
                    .collect();
                return Ok(Json(
                    json!({"publicRead": hosted.public_read, "writers": writers, "grants": grants}),
                )
                .into_response());
            }
            if part != "repo" {
                return Err(invalid(format!(
                    "part is meta, repo, blobs or blob, not {part}"
                )));
            }
            let head = host
                .store
                .space_head(group, space)
                .map_err(XrpcError::internal)?;
            let records = host
                .store
                .space_records(group, space)
                .map_err(XrpcError::internal)?;
            // A space of the group's always holds its access record.
            let head = head.unwrap_or(crate::store::SpaceHead {
                rev: String::new(),
                state: LtHash::default().state(),
            });
            let data = LtHash::from_state(&head.state)
                .expect("a state the host wrote")
                .digest();
            let rev = head.rev;
            let commit = atproto::space::Commit {
                repo: group.into(),
                space: space.into(),
                rev,
                data,
            };
            Ok((
                [(header::CONTENT_TYPE, "application/vnd.dasl.star")],
                star(&commit, &records),
            )
                .into_response())
        }
    }
}

/// The record at `path`, `collection/rkey`, of `value`'s DRISL, to write.
pub(crate) fn create(path: &str, value: &[u8]) -> Option<Write> {
    let (collection, rkey) = path.split_once('/')?;
    let value = drisl::decode(value).ok()?.to_json();
    Some(Write::Create {
        collection: collection.into(),
        rkey: rkey.into(),
        value,
    })
}

/// The records of a repo's CAR, once its commit is `group`'s, signed by
/// `key`, and its tree and records are whole.
pub(crate) fn repo_writes(
    bytes: &[u8],
    group: &str,
    key: &atproto::PublicKey,
) -> Option<Vec<Write>> {
    let repo = car::read(bytes).ok()?;
    let commit = atproto::Commit::from_bytes(repo.blocks.get(repo.roots.first()?)?).ok()?;
    (commit.did == group && commit.verify(key)).then_some(())?;
    let tree = Tree::read(commit.data, &repo.blocks).ok()?;
    let leaves = tree.leaves();
    leaves
        .into_iter()
        .map(|(path, cid)| create(path, repo.blocks.get(&cid)?))
        .collect()
}

/// The records of `group`'s repo in `space`, from its STAR archive, once
/// they are what its commit's set hash says.
fn space_writes(bytes: &[u8], group: &str, space: &str) -> Option<Vec<Write>> {
    let (commit, held) = read_star(bytes).ok()?;
    let mut hash = LtHash::default();
    for (path, value) in &held {
        let (collection, rkey) = path.split_once('/')?;
        hash.add(&element(collection, rkey, &Cid::of_cbor(value)));
    }
    let whole = hash.digest() == commit.data && commit.repo == group && commit.space == space;
    whole.then_some(())?;
    held.iter()
        .map(|(path, value)| create(path, value))
        .collect()
}

/// The handle a group arriving with `handle` (and `address`, the handle
/// it had under the host it leaves) takes under `domain`, and its address
/// here: the same label under this host's domain, and a domain of its own
/// kept as its handle.
pub fn handle_here(handle: &str, address: Option<&str>, domain: &str) -> (String, String) {
    let label = address
        .unwrap_or(handle)
        .split('.')
        .next()
        .unwrap_or_default();
    let here = format!("{label}{domain}");
    let was = address.unwrap_or(handle);
    let own = handle != was;
    (
        if own {
            handle.to_string()
        } else {
            here.clone()
        },
        here,
    )
}

impl Host {
    /// GETs `part` of the group's export from its host at `pds`.
    async fn pull(
        &self,
        pds: &str,
        group: &str,
        token: &str,
        extra: &[(&str, &str)],
    ) -> Result<reqwest::Response, XrpcError> {
        let url = format!("{pds}/xrpc/fyi.opensocial.migrateExport");
        let mut query = vec![("group", group), ("token", token)];
        query.extend_from_slice(extra);
        self.ask_source(&url, &query).await
    }

    /// GETs `url` of the group's host, with `query`: an answer other than
    /// a success is the host's to say, not read as one.
    async fn ask_source(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::Response, XrpcError> {
        let (pinned, at) = self.pinned(url).await.map_err(source)?;
        let res = pinned.get(at).query(query).send().await.map_err(source)?;
        if !res.status().is_success() {
            return Err(source(format!("{url} answered {}", res.status())));
        }
        Ok(res)
    }
}

/// A new account here for `did`, at `address`, known by `handle`, with
/// keys of its own, as one arriving from another server is until the
/// directory names this host: its records follow it in.
pub(crate) async fn take_in(
    host: &Host,
    did: &str,
    handle: &str,
    address: &str,
) -> Result<Account, XrpcError> {
    let account = Account {
        did: did.into(),
        handle: address.into(),
        signing: Keypair::generate(Curve::K256),
        kem: Keypair::generate(Curve::P256),
    };
    let empty = host.sign(&account, &Tree::build(&BTreeMap::new()));
    let made = host
        .store
        .create_account(&account, &empty, now_millis() / 1000, &[]);
    if !made.map_err(XrpcError::internal)? {
        return Err(refused(
            "HandleNotAvailable",
            format!("{address} is taken here"),
        ));
    }
    if handle != address {
        let named = host.store.set_handle(did, handle, &[]);
        if named.is_err() {
            host.forget(did);
            return Err(refused(
                "HandleNotAvailable",
                format!("{handle} is taken here"),
            ));
        }
    }
    Ok(Account {
        handle: handle.into(),
        ..account
    })
}

/// What an account's PLC operation should name for it to live here, as
/// com.atproto.identity.getRecommendedDidCredentials answers it.
pub(crate) fn creds(host: &Host, account: &Account) -> Value {
    json!({
        "rotationKeys": [host.secrets.rotation.public().did_key()],
        "alsoKnownAs": [format!("at://{}", account.handle)],
        "verificationMethods": {
            "atproto": account.signing.public().did_key(),
            "atproto_kem": account.kem.public().did_key(),
        },
        "services": {"atproto_pds": {"type": "AtprotoPersonalDataServer", "endpoint": host.config.public_url}},
    })
}

impl Host {
    /// Whether `handle` names `did`, as the resolvers this host asks say.
    pub(crate) async fn names(&self, handle: &str, did: &str) -> bool {
        let resolvers = &self.config.handle_resolvers;
        let named = self.identity.resolve_handle(handle, resolvers).await;
        named.as_deref() == Some(did)
    }

    /// An account taken in part way, and its files, gone.
    pub(crate) fn forget(&self, did: &str) {
        let gone = self.store.delete_account(did, &[]);
        let forgotten = gone.is_ok();
        tracing::info!(did, forgotten, "an account taken in part way");
        let blobs = gone.unwrap_or_default();
        self.remove_blobs(blobs.iter().map(|cid| (did, cid)));
    }
}

/// Step 2, at the destination: the group taken in, switched off; what
/// the PLC operation should name.
pub async fn migrate_receive(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.migrateReceive")).await?;
    host.founding(&me)?;
    let group = body["did"].as_str().expect("its lexicon requires it");
    let token = body["exportToken"]
        .as_str()
        .expect("its lexicon requires it");
    let jwt = body["serviceJwt"]
        .as_str()
        .expect("its lexicon requires it");
    if host
        .store
        .account(group)
        .map_err(XrpcError::internal)?
        .is_some()
    {
        return Err(refused("AlreadyHere", "the group is on this host already"));
    }
    // The group itself says it is moving here, by its current key.
    let doc = host.identity.document(group).await.map_err(source)?;
    let (Ok(key), Some(pds)) = (did::signing_key(&doc), did::pds(&doc)) else {
        return Err(source("the group's DID names no key, or no host"));
    };
    let now = (now_millis() / 1000) as u64;
    let checked = service_auth::verify(jwt, &key, now).and_then(|claims| {
        service_auth::check(&claims, &host.config.service_did, RECEIVE).map(String::from)
    });
    if checked.as_deref() != Ok(group) {
        return Err(refused(
            "BadServiceJwt",
            "not the group's token for moving here",
        ));
    }
    let snapshot = &body["snapshot"];
    let handle = snapshot["handle"]
        .as_str()
        .ok_or_else(|| invalid("snapshot.handle is required"))?;
    let domain = &host.config.handle_domain;
    let (handle, here) = handle_here(handle, snapshot["address"].as_str(), domain);
    // A domain of its own comes along only while it still names the group:
    // the snapshot's word for it is not enough.
    let handle = if handle == here || host.names(&handle, group).await {
        handle
    } else {
        here.clone()
    };

    // Its public repo, checked against the group's key.
    let url = format!("{pds}/xrpc/com.atproto.sync.getRepo");
    let res = host.ask_source(&url, &[("did", group)]).await?;
    let read = capped(res, REPO_AT_MOST).await;
    let bytes = read.ok_or(source(too_much(REPO_AT_MOST)))?;
    let writes = repo_writes(&bytes, group, &key);
    let writes = writes.ok_or(source("a repo that is not the group's, signed and whole"))?;

    let account = take_in(&host, group, &handle, &here).await?;
    let received = receive(&host, group, &me, &pds, token, snapshot, writes).await;
    let counts = match received {
        Ok(counts) => counts,
        Err(e) => {
            // Nothing half taken in stays.
            host.forget(group);
            tracing::warn!(group, error = ?e, "a group not taken in");
            return Err(e);
        }
    };
    tracing::info!(group, from = pds, "a group taken in, switched off");
    let creds = creds(&host, &account);
    Ok(Json(
        json!({"recommendedCreds": creds, "records": counts.0, "blobs": counts.1, "spaces": counts.2}),
    ))
}

/// Takes the group's files, repo and spaces in, as `me`, who must be one
/// who configures it by its own records. How many records, files and
/// spaces.
async fn receive(
    host: &Host,
    group: &str,
    me: &str,
    pds: &str,
    token: &str,
    snapshot: &Value,
    writes: Vec<Write>,
) -> Result<(usize, usize, usize), XrpcError> {
    host.store
        .deactivate(group, now_millis(), me, &[], &[])
        .map_err(XrpcError::internal)?;
    let off = Asked {
        off: true,
        ..Asked::default()
    };

    // Its files first: its records hold them.
    let res = host.pull(pds, group, token, &[("part", "blobs")]).await?;
    let listed: Value = json_at_most(res).await?;
    let blobs = listed["blobs"].as_array().cloned().unwrap_or_default();
    for blob in &blobs {
        let cid = blob["cid"].as_str().unwrap_or_default();
        let res = host
            .pull(pds, group, token, &[("part", "blob"), ("cid", cid)])
            .await?;
        let most = host.config.blob_upload_limit as usize;
        let read = capped(res, most).await;
        let bytes = read.ok_or(source(too_much(most)))?;
        if Cid::of_raw(&bytes).to_string() != cid {
            return Err(source(format!("the file {cid} is not what it says")));
        }
        host.upload_blob(group, &bytes, blob["mimeType"].as_str().unwrap_or_default())?;
    }

    let records = writes.len();
    host.write_asked(group, &writes, &off)?;

    // Each space: its authority's state, and the group's repo there,
    // checked against its own commit.
    let spaces: Vec<String> = snapshot["spaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(String::from))
        .collect();
    for uri in &spaces {
        let space = SpaceRef::parse(uri).map_err(|e| invalid(e.to_string()))?;
        if space.authority != group {
            return Err(invalid(format!("{uri} is not one of the group's spaces")));
        }
        let res = host
            .pull(pds, group, token, &[("part", "meta"), ("space", uri)])
            .await?;
        let meta: Value = json_at_most(res).await?;
        host.create_space(group, &space.kind, &space.skey, meta["publicRead"] == true)?;
        let res = host
            .pull(pds, group, token, &[("part", "repo"), ("space", uri)])
            .await?;
        let read = capped(res, REPO_AT_MOST).await;
        let bytes = read.ok_or(source(too_much(REPO_AT_MOST)))?;
        let writes = space_writes(&bytes, group, uri);
        let writes = writes.ok_or(source(format!(
            "the group's repo in {uri} is not what its commit says"
        )))?;
        host.space_write_asked(group, uri, &writes, &off).await?;
        for w in meta["writers"].as_array().into_iter().flatten() {
            let data = w["data"].as_str().and_then(jose::unb64).unwrap_or_default();
            let (who, rev) = (
                w["did"].as_str().unwrap_or_default(),
                w["repoRev"].as_str().unwrap_or_default(),
            );
            host.record_writer(uri, who, rev, &data)
                .map_err(XrpcError::internal)?;
        }
        for g in meta["grants"].as_array().into_iter().flatten() {
            let who = g["did"].as_str().unwrap_or_default();
            host.grant(uri, who, g["read"] == true, g["write"] == true)?;
        }
    }

    // Only one who may configure the group, by its own records, takes it.
    if !host.may_do(group, me, "group.configure")? {
        return Err(XrpcError::new(
            StatusCode::FORBIDDEN,
            "Forbidden",
            "only one who configures the group may move it",
        ));
    }
    Ok((records, blobs.len(), spaces.len()))
}

/// Step 3, at the source: the operation naming the destination signed and
/// sent, its stewards' keys kept ahead, and the group switched off here.
pub async fn migrate_complete(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, me) = host
        .mover(&headers, "migrateComplete", body["group"].as_str())
        .await?;
    let creds = &body["recommendedCreds"];
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(String::from))
            .collect()
    };
    let endpoint = creds["services"]["atproto_pds"]["endpoint"].as_str();
    let endpoint = endpoint.ok_or_else(|| invalid("recommendedCreds names no host"))?;
    let signing = creds["verificationMethods"]["atproto"].as_str();
    let signing = signing.ok_or_else(|| invalid("recommendedCreds names no signing key"))?;
    let theirs = strings(&creds["rotationKeys"]);
    if theirs.is_empty() {
        return Err(invalid("recommendedCreds names no rotation key"));
    }
    let turn = host.plc.turn().await;
    let last = host
        .plc
        .last_op(&group)
        .await
        .map_err(XrpcError::internal)?;
    let ours = host.secrets.rotation.public().did_key();
    let atproto::plc::Signed::Update { op, .. } = &last else {
        return Err(refused("NotController", "the group's DID is ended"));
    };
    if !op.rotation_keys.contains(&ours) {
        return Err(refused(
            "NotController",
            "this host no longer holds a rotation key for the group",
        ));
    }
    let mut next = op.clone();
    // Its stewards keep their keys, ahead of the new host's.
    next.rotation_keys = op
        .rotation_keys
        .iter()
        .filter(|k| **k != ours)
        .cloned()
        .chain(theirs)
        .collect();
    next.verification_methods = creds["verificationMethods"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    next.verification_methods
        .insert("atproto".into(), signing.into());
    let aka = strings(&creds["alsoKnownAs"]);
    if !aka.is_empty() {
        next.also_known_as = aka;
    }
    next.services.insert(
        "atproto_pds".into(),
        atproto::plc::Service {
            kind: "AtprotoPersonalDataServer".into(),
            endpoint: endpoint.into(),
        },
    );
    let signed = next.sign(Some(last.cid()), &host.secrets.rotation);
    host.plc
        .publish(&group, &signed)
        .await
        .map_err(crate::plc::not_published)?;
    drop(turn);
    let off = host.store.deactivate(&group, now_millis(), &me, &[], &[]);
    off.map_err(XrpcError::internal)?;
    // Handed over, there is nothing more to pull.
    host.store.end_export(&group).map_err(XrpcError::internal)?;
    host.record(&group, &me, None, &format!("moved the group to {endpoint}"))
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({ "movedTo": endpoint })))
}

/// Step 4, at the destination: once the directory names this host, the
/// group switched on.
pub async fn migrate_activate(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, _) = host
        .mover(&headers, "migrateActivate", body["did"].as_str())
        .await?;
    let doc = host.identity.document(&group).await.map_err(source)?;
    let account = host
        .store
        .account(&group)
        .map_err(XrpcError::internal)?
        .expect("a group here");
    let here = did::pds(&doc).as_deref() == Some(host.config.public_url.as_str());
    let signs = did::signing_key(&doc).is_ok_and(|k| k == account.signing.public());
    if !here || !signs {
        return Err(refused(
            "NotYet",
            "the directory does not name this host for the group yet",
        ));
    }
    let on = host.activate_arrived(&group, &account.handle);
    on.map_err(XrpcError::internal)?;
    // Its repos in spaces were written while it was off, which no space
    // hears of: each is told now, as an account brought here is.
    let told = host.tell_spaces(&group).await;
    told.map_err(XrpcError::internal)?;
    let spaces = host.store.spaces_of(&group).map_err(XrpcError::internal)?;
    for space in &spaces {
        host.reindex(space);
    }
    Ok(Json(json!({ "spaces": spaces.len() })))
}
