//! An account's spaces, brought with it as it moves here to become a
//! group (`import.rs`): while its move is pending, whoever started it
//! hands each over as the account's own server serves it, since only the
//! account, signed in there, may export them.
//!
//! - `meta`: a space it governs, as the space alpha's meta archive
//!   (`com.atproto.space.exportSpaceMeta`): its policy, its members, and
//!   who writes there. Before any repo in it.
//! - `repo`: its own repo in a space, its or another's, as a STAR archive
//!   (`com.atproto.space.getRepo`), checked against its commit's set hash.
//! - `blob`: one of its files in a space, before the repos that hold it.
//!
//! A space's policy becomes this host's over it, `fyi.opensocial.policy`,
//! left out of the group's index: a memberspace's public reading its
//! `publicRead`, and its members, each kept as an account's grant beside
//! the policy (reading, writing). Anything this host would not do as the
//! policy says (anyone writing, only some apps, a managing app) refuses
//! the space rather than quietly widening or narrowing who uses it. A
//! space already under `fyi.opensocial.policy`, as a group moving between
//! hosts brings its spaces, keeps its policy as it is.
//!
//! Its repos tell their spaces' authorities nothing while the account is
//! switched off: an authority elsewhere would check the account's word
//! against the key the directory still names, the old server's. Once
//! `completeImport` switches it on here, each is told, by its key here.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::group::{members_space, meta_space};
use crate::group_api::signed_in;
use crate::lexicons::lexicons;
use crate::migration::create;
use crate::rbac::{NS, POLICY};
use crate::repo::{Asked, Write, record_cid};
use crate::store::{SpaceHead, Store};
use crate::xrpc::{XrpcError, read_body};
use anyhow::Result;
use atproto::Cid;
use atproto::space::{Commit, LtHash, SpaceMeta, SpaceRef, element, read_space_meta, read_star};
use atproto::{Value, syntax};
use axum::Json;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use rusqlite::params;
use serde_json::json;
use std::sync::Arc;

/// The most an archive brought in may be, in bytes; a file is held to
/// `BLOB_UPLOAD_LIMIT`.
pub const ARCHIVE_AT_MOST: u64 = 100 * 1024 * 1024;

const PERSONAL: &str = "com.atproto.personalspace.policy";
const MEMBERS: &str = "com.atproto.memberspace.policy";
/// An account's grant in a space beside its policy, as a space meta
/// archive's policy state keeps it.
pub(crate) const MEMBER: &str = "com.atproto.memberspace.member";
const MEMBER_PREFIX: &str = "com.atproto.memberspace.member/";
const SPACE_REPOS: &str = "SELECT space, rev, state FROM space_repo WHERE did = ?1 ORDER BY space";

/// A DID let into a space: whether it may read, and write.
pub type Grant = (String, bool, bool);
/// A repo in a space's writer set: its DID, revision and digest, and the
/// space's revision it is kept at.
pub type Writer = (String, String, Vec<u8>, String);

fn refused(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

fn unsupported(why: &str) -> XrpcError {
    refused(
        "UnsupportedPolicy",
        format!("{why}: this host would not keep the space as its policy says"),
    )
}

/// A member's DID from its record key, as the PDS writes it: a localhost
/// did:web's port, which a record key cannot hold encoded, unencoded.
pub fn record_key_to_did(rkey: &str) -> String {
    let port = rkey
        .strip_prefix("did:web:")
        .and_then(|rest| rest.split_once(':'))
        .filter(|(host, port)| {
            !host.is_empty() && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
        });
    match port {
        Some((host, port)) => format!("did:web:{host}%3A{port}"),
        None => rkey.to_string(),
    }
}

/// What this host keeps of the space `meta` holds: its policy, as ours,
/// and each account's grant beside it (reading, writing).
pub(crate) fn kept(meta: &SpaceMeta) -> Result<(serde_json::Value, Vec<Grant>), XrpcError> {
    let flag = |key: &str| meta.policy.get(key) == Some(&Value::Bool(true));
    let kind = meta.policy.get("$type");
    let (ours, members) = match kind {
        Some(Value::String(t)) if t == POLICY => (true, true),
        Some(Value::String(t)) if t == MEMBERS => (false, true),
        Some(Value::String(t)) if t == PERSONAL => (false, false),
        _ => {
            return Err(unsupported(
                "a policy other than personalspace, memberspace or this host's own",
            ));
        }
    };
    if flag("publicWrite") {
        return Err(unsupported("anyone writing in it"));
    }
    if meta.policy.contains_key("allowedApps") {
        return Err(unsupported("only some apps using it"));
    }
    let grants = meta.policy_state.iter().map(|(key, value)| {
        let did = key.strip_prefix(MEMBER_PREFIX).map(record_key_to_did);
        let did = did.filter(|d| members && syntax::is_did(d));
        let (Some(did), Some(Value::Bool(read)), Some(Value::Bool(write))) =
            (did, value.get("read"), value.get("write"))
        else {
            return Err(refused(
                "InvalidArchive",
                format!("{key} is no member of the space"),
            ));
        };
        Ok((did, *read, *write))
    });
    let grants = grants.collect::<Result<_, _>>()?;
    let policy = Value::Map(meta.policy.clone()).to_json();
    if ours {
        let checked = lexicons().validate(POLICY, &policy);
        checked.map_err(|e| refused("InvalidArchive", e.to_string()))?;
        return Ok((policy, grants));
    }
    let mut policy = json!({"$type": POLICY, "unlisted": true, "grants": []});
    if members && flag("publicRead") {
        policy["publicRead"] = json!(true);
    }
    Ok((policy, grants))
}

impl Store {
    /// Keeps the space at `uri`, of `authority`'s, as an archive brought it,
    /// replacing what was kept for it: its policy, `grants` beside it,
    /// and `writers`, each with the space's revision it is kept at.
    pub fn import_space(
        &self,
        uri: &str,
        authority: &str,
        policy: &serde_json::Value,
        grants: &[Grant],
        writers: &[Writer],
        now: i64,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO space (uri, authority, public_read, created_at, policy)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (uri) DO UPDATE SET public_read = excluded.public_read,
             policy = excluded.policy",
            params![
                uri,
                authority,
                policy["publicRead"] == true,
                now,
                policy.to_string()
            ],
        )?;
        tx.execute("DELETE FROM space_grant WHERE space = ?1", [uri])?;
        tx.execute("DELETE FROM space_writer WHERE space = ?1", [uri])?;
        for (did, read, write) in grants {
            tx.execute(
                "INSERT INTO space_grant (space, did, read, write) VALUES (?1, ?2, ?3, ?4)",
                params![uri, did, read, write],
            )?;
        }
        for (did, rev, data, space_rev) in writers {
            tx.execute(
                "INSERT INTO space_writer (space, did, repo_rev, data, space_rev)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![uri, did, rev, data, space_rev],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// `did`'s repos in spaces: each space, and the repo's head there.
    pub fn space_repos(&self, did: &str) -> Result<Vec<(String, SpaceHead)>> {
        let conn = self.conn();
        let mut q = conn.prepare(SPACE_REPOS)?;
        let rows = q.query_map([did], |r| {
            let head = SpaceHead {
                rev: r.get(1)?,
                state: r.get(2)?,
            };
            Ok((r.get(0)?, head))
        });
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl Host {
    /// A space `did` governs, from its meta archive: kept here as this
    /// host would keep it, replacing what an earlier try kept.
    pub(crate) fn import_space_meta(&self, did: &str, bytes: &[u8]) -> Result<String, XrpcError> {
        let meta = read_space_meta(bytes);
        let meta = meta.map_err(|e| refused("InvalidArchive", e.to_string()))?;
        // Its group makes these two as it is made.
        if [meta_space(did), members_space(did)].contains(&meta.space) {
            return Err(refused(
                "SpaceReserved",
                format!("{} is the group's own, made with it", meta.space),
            ));
        }
        self.keep_space_meta(did, meta)
    }

    /// A space `did` governs, as `meta` holds it: kept here as this host
    /// would keep it, replacing what was kept of it. Its URI.
    pub(crate) fn keep_space_meta(&self, did: &str, meta: SpaceMeta) -> Result<String, XrpcError> {
        let space = SpaceRef::parse(&meta.space).expect("a space the archive's reader checked");
        if space.authority != did {
            return Err(refused("NotSpaceOwner", "that space isn't the account's"));
        }
        let (policy, grants) = kept(&meta)?;
        // Its own repo there comes as a repo of its own, and is told to
        // the space as the account is switched on.
        let writers: Vec<_> = meta
            .writers
            .into_iter()
            .filter(|w| w.did != did)
            .map(|w| (w.did, w.repo_rev, w.data, self.clock.next()))
            .collect();
        let now = crate::time::now_millis();
        let kept = self
            .store
            .import_space(&meta.space, did, &policy, &grants, &writers, now);
        kept.map_err(XrpcError::internal)?;
        Ok(meta.space)
    }

    /// `did`'s repo in a space, from its STAR archive once its records are
    /// what its commit says: kept here in place of what an earlier try
    /// kept, less any record this host would not keep. How many it kept.
    pub(crate) fn import_space_repo(
        &self,
        did: &str,
        bytes: &[u8],
    ) -> Result<(String, usize), XrpcError> {
        let read = read_star(bytes).map_err(|e| refused("InvalidArchive", e.to_string()));
        let (commit, records) = read?;
        if commit.repo != did {
            return Err(refused("NotSpaceOwner", "that repo isn't the account's"));
        }
        let space = SpaceRef::parse(&commit.space);
        let space = space.map_err(|e| refused("InvalidArchive", e.to_string()))?;
        let uri = commit.space;
        let governed = self.store.space(&uri).map_err(XrpcError::internal)?;
        if space.authority == did && governed.is_none() {
            return Err(refused(
                "NotFound",
                format!("{uri} is the account's: bring its meta before its repos"),
            ));
        }
        let mut hash = LtHash::default();
        let mut writes = Vec::new();
        for (path, value) in &records {
            // `read_star` takes only `collection/rkey` keys.
            let (collection, rkey) = path.split_once('/').expect("a record path");
            hash.add(&element(collection, rkey, &Cid::of_cbor(value)));
            writes.extend(create(path, value).filter(keeps));
        }
        if hash.digest() != commit.data {
            return Err(refused(
                "InvalidArchive",
                "its records are not what its commit says",
            ));
        }
        let kept = writes.len();
        let held = self.store.space_paths(did, &uri);
        let gone = held.map_err(XrpcError::internal)?;
        let gone = gone.keys().filter(|p| !records.contains_key(*p)).map(|p| {
            let (collection, rkey) = p.split_once('/').expect("a path the host keeps");
            Write::Delete {
                collection: collection.into(),
                rkey: rkey.into(),
            }
        });
        writes.extend(gone);
        let copied = Asked {
            put: true,
            off: true,
            copied: true,
            ..Asked::default()
        };
        self.space_commit(did, &space, &writes, &copied)?;
        Ok((uri, kept))
    }

    /// Tells each space `did` has a repo in of it, as `did` is switched on
    /// here.
    pub(crate) async fn tell_spaces(&self, did: &str) -> Result<(), XrpcError> {
        let repos = self.store.space_repos(did).map_err(XrpcError::internal)?;
        for (space, head) in repos {
            let parsed = SpaceRef::parse(&space).expect("a space the host keeps");
            let data = LtHash::from_state(&head.state).expect("a state the host wrote");
            let commit = Commit {
                repo: did.into(),
                space,
                rev: head.rev,
                data: data.digest(),
            };
            // The authority not hearing of it fails nothing, as with any write.
            if let Err(e) = self.tell_authority(&parsed, &commit).await {
                let (space, error) = (commit.space, format!("{e:#}"));
                tracing::warn!(space, repo = did, error, "telling the authority failed");
            }
        }
        Ok(())
    }

    /// Publishes the policy of each space `did` governs, listing it in its
    /// index as the policy says, and reads it again: once its members
    /// space, which holds the index, is there.
    pub(crate) async fn open_spaces(&self, did: &str) -> Result<(), XrpcError> {
        let governed = self.store.spaces_of(did).map_err(XrpcError::internal)?;
        for space in governed {
            let policy = self.hosted_space(&space)?.policy;
            self.publish_policy(did, &space, &policy).await?;
            self.reindex(&space);
        }
        Ok(())
    }
}

/// Whether this host keeps `write`'s record: one by the data model, of
/// its collection's type.
pub(crate) fn keeps(write: &Write) -> bool {
    matches!(write, Write::Create { collection, value, .. } if record_cid(collection, value).is_ok())
}

/// One of a moving account's spaces, its repo in one, or a file there, for
/// whoever started its move, while it is pending.
pub async fn import_account_space(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<serde_json::Value>, XrpcError> {
    let me = signed_in(&host, &headers, &format!("{NS}.importAccountSpace")).await?;
    let did = param(&p, "did")?;
    let pending = host
        .store
        .pending_import(did)
        .map_err(XrpcError::internal)?;
    if !pending.is_some_and(|p| p.founder == me) {
        return Err(refused("NotFound", "no move of that account here of yours"));
    }
    let part = param(&p, "part")?;
    let limit = match part {
        "meta" | "repo" => ARCHIVE_AT_MOST,
        "blob" => host.config.blob_upload_limit,
        _ => return Err(invalid(format!("part is meta, repo or blob, not {part}"))),
    };
    let bytes = read_body(body, limit, "TooLarge").await?;
    match part {
        "meta" => Ok(Json(
            json!({ "space": host.import_space_meta(did, &bytes)? }),
        )),
        "repo" => {
            let (space, records) = host.import_space_repo(did, &bytes)?;
            Ok(Json(json!({ "space": space, "records": records })))
        }
        _ => {
            let mime = headers.get(header::CONTENT_TYPE);
            let mime = mime.and_then(|v| v.to_str().ok()).unwrap_or_default();
            let blob = host.upload_blob(did, &bytes, mime)?;
            Ok(Json(json!({ "cid": blob["ref"]["$link"] })))
        }
    }
}
