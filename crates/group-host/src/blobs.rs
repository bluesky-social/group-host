//! Blobs: files on disk under `DATA_DIR/blobs`, named by CID in each
//! account's own directory. An upload is temporary; a write whose record
//! references it makes it permanent, and one that drops the last reference
//! deletes it. Temporary blobs no record took within a day are swept. Only
//! a blob a record references is served.
//!
//! An image of an account's is also served for a page to draw, at
//! `/img/{did}/{cid}`, to whoever may see a record that holds it: anyone,
//! for a record in its public repo; for one in a space, whoever signed in
//! to the console that space lets read, a group's listed profile being for
//! anyone signed in; but none a takedown of the group's takes out of the
//! space (`takedown.rs`), to anyone. Served as an image and nothing more,
//! and kept for good, as a CID names its bytes.

use crate::Host;
use crate::authority::Access;
use crate::store::{Blob, Store};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::Cid;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// How long an upload waits for a record to reference it.
pub const TEMPORARY_FOR: i64 = 24 * 60 * 60 * 1000;

impl Host {
    fn blob_path(&self, did: &str, cid: &Cid) -> PathBuf {
        let dir = did.replace(':', "_");
        self.config
            .data_dir
            .join("blobs")
            .join(dir)
            .join(cid.to_string())
    }

    /// Keeps `bytes` as a temporary blob of `did`'s, answering the
    /// reference a record holds it by.
    pub fn upload_blob(
        &self,
        did: &str,
        bytes: &[u8],
        mime_type: &str,
    ) -> Result<Value, XrpcError> {
        let account = self.store.account(did).map_err(XrpcError::internal)?;
        if account.is_none() {
            return Err(XrpcError::new(
                StatusCode::BAD_REQUEST,
                "RepoNotFound",
                format!("no repo {did} here"),
            ));
        }
        let limit = self.config.blob_upload_limit;
        if bytes.len() as u64 > limit {
            let said = format!("a blob is {limit} bytes at most");
            return Err(XrpcError::new(
                StatusCode::BAD_REQUEST,
                "BlobTooLarge",
                said,
            ));
        }
        let cid = Cid::of_raw(bytes);
        let mime_type = if mime_type.is_empty() {
            "application/octet-stream"
        } else {
            mime_type
        };
        self.write_blob(did, &cid, bytes)
            .map_err(XrpcError::internal)?;
        let kept = self
            .store
            .put_blob(did, &cid, mime_type, bytes.len() as i64, now_millis());
        kept.map_err(XrpcError::internal)?;
        Ok(
            json!({"$type": "blob", "ref": {"$link": cid.to_string()}, "mimeType": mime_type, "size": bytes.len()}),
        )
    }

    /// Writes the file whole or not at all: a reader never sees part of it.
    fn write_blob(&self, did: &str, cid: &Cid, bytes: &[u8]) -> std::io::Result<()> {
        let path = self.blob_path(did, cid);
        std::fs::create_dir_all(path.parent().expect("a blob's file is in a directory"))?;
        let part = path.with_extension(format!("{}.part", std::process::id()));
        std::fs::write(&part, bytes)?;
        std::fs::rename(&part, &path)
    }

    /// `did`'s blob `cid`, and its bytes, once a record references it.
    pub fn read_blob(&self, did: &str, cid: &Cid) -> anyhow::Result<Option<(Blob, Vec<u8>)>> {
        let Some(blob) = self.store.blob(did, cid)?.filter(|b| !b.temporary) else {
            return Ok(None);
        };
        let bytes = std::fs::read(self.blob_path(did, cid))?;
        Ok(Some((blob, bytes)))
    }

    /// Deletes the files of blobs the database has let go.
    pub(crate) fn remove_blobs<'a>(&self, blobs: impl IntoIterator<Item = (&'a str, &'a Cid)>) {
        for (did, cid) in blobs {
            let _ = std::fs::remove_file(self.blob_path(did, cid));
        }
    }

    /// Removes the uploads no record took before `now` less a day, saying
    /// how many.
    pub fn sweep_blobs(&self, now: i64) -> anyhow::Result<usize> {
        let swept = self.store.sweep_blobs(now - TEMPORARY_FOR)?;
        self.remove_blobs(swept.iter().map(|(did, cid)| (did.as_str(), cid)));
        Ok(swept.len())
    }
}

/// The blobs a record's JSON references: each `{"$type": "blob", "ref":
/// {"$link": cid}}` in it, at any depth.
pub fn blob_refs(record: &Value) -> Vec<Cid> {
    let mut found = Vec::new();
    collect(record, &mut found);
    found.sort();
    found.dedup();
    found
}

fn collect(value: &Value, found: &mut Vec<Cid>) {
    match value {
        Value::Array(items) => items.iter().for_each(|v| collect(v, found)),
        Value::Object(map) => {
            let link = map
                .get("ref")
                .and_then(|r| r.get("$link"))
                .and_then(Value::as_str);
            if map.get("$type").and_then(Value::as_str) == Some("blob")
                && let Some(cid) = link.and_then(|l| Cid::parse(l).ok())
            {
                found.push(cid);
            }
            map.values().for_each(|v| collect(v, found));
        }
        _ => {}
    }
}

/// The kinds of image a page may be given.
pub(crate) const IMAGES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/avif",
];

/// The kinds of image a group's avatar and banner may be.
pub(crate) const PROFILE_IMAGES: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

impl Store {
    /// Whether a record in `did`'s public repo holds the blob `cid`.
    pub fn held_publicly(&self, did: &str, cid: &Cid) -> anyhow::Result<bool> {
        let sql = "SELECT 1 FROM record_blob WHERE did = ?1 AND cid = ?2 LIMIT 1";
        let found = self
            .conn()
            .query_row(sql, [did, &cid.to_string()], |_| Ok(()));
        Ok(found.optional()?.is_some())
    }

    /// The spaces in which a record of `did`'s holds the blob `cid`.
    pub fn holding_spaces(&self, did: &str, cid: &Cid) -> anyhow::Result<Vec<String>> {
        let conn = self.conn();
        let sql = "SELECT DISTINCT space FROM space_record_blob WHERE did = ?1 AND cid = ?2";
        let mut q = conn.prepare(sql)?;
        let rows = q.query_map([did, &cid.to_string()], |r| r.get(0));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl Host {
    /// Whether `who` (none, for someone not signed in) may see `did`'s blob
    /// `cid`: a record holding it is one they may read. Someone not signed
    /// in sees only what the public repo holds, as of a group they learn
    /// no more than its DID and handle. In a space, none the group took
    /// down (`takedown.rs`), to anyone, as `getSpaceBlob` serves it.
    pub fn may_see(&self, did: &str, cid: &Cid, who: Option<&str>) -> Result<bool> {
        if self.store.deactivation(did)?.is_some() {
            return Ok(false);
        }
        if self.store.held_publicly(did, cid)? {
            return Ok(true);
        }
        let Some(who) = who else {
            return Ok(false);
        };
        for space in self.store.holding_spaces(did, cid)? {
            let hosted = self.store.space(&space)?;
            let may = hosted
                .as_ref()
                .map(|h| self.may(&space, h, Some(who), Access::Read));
            if !may.transpose()?.unwrap_or(false) {
                continue;
            }
            let group = &hosted.expect("a space they read is hosted").authority;
            let down = self.taken_down(group, &space)?;
            if !self.file_taken_down(&down, &space, did, cid)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// An image of `did`'s, `cid`, for whoever may see it: nothing for anyone
/// else, or for a blob that is not an image.
pub async fn image(
    State(host): State<Arc<Host>>,
    Path((did, cid)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let gone = || StatusCode::NOT_FOUND.into_response();
    let Ok(cid) = Cid::parse(&cid) else {
        return gone();
    };
    let who = host.console_caller(&headers).ok().flatten();
    if !host.may_see(&did, &cid, who.as_deref()).unwrap_or(false) {
        return gone();
    }
    let read = host.read_blob(&did, &cid).ok().flatten();
    let Some((blob, bytes)) = read.filter(|(b, _)| IMAGES.contains(&b.mime_type.as_str())) else {
        return gone();
    };
    let headers = [
        (header::CONTENT_TYPE, blob.mime_type),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox".into(),
        ),
        // Whoever else may see it, a CID's bytes never change: kept in a
        // browser, not in a cache between.
        (
            header::CACHE_CONTROL,
            "private, max-age=31536000, immutable".into(),
        ),
    ];
    (headers, bytes).into_response()
}
