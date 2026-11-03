//! Blobs: files on disk under `DATA_DIR/blobs`, named by CID in each
//! account's own directory. An upload is temporary; a write whose record
//! references it makes it permanent, and one that drops the last reference
//! deletes it. Temporary blobs no record took within a day are swept. Only
//! a blob a record references is served.

use crate::Host;
use crate::store::Blob;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::Cid;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::path::PathBuf;

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
