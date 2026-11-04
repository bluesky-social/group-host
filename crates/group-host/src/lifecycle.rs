//! An account switched off, back on, and deleted, as a PDS does to an
//! account, and the relays told of each.
//!
//! While it is off, nothing reads its repos or its spaces and nothing
//! writes as it; who may switch it back on is kept with it, as nothing
//! can read its memberships to say. Deleted, it is gone from the host, and
//! its DID ended in the directory.

use crate::Host;
use crate::repo::event;
use crate::store::Deactivation;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use anyhow::{Context, Result};
use atproto::Value;
use axum::http::StatusCode;

/// The refusal of a repo whose account is switched off.
pub fn deactivated(did: &str) -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "RepoDeactivated",
        format!("{did} is deactivated"),
    )
}

/// An `#account` event: whether it is active, and if not, why.
fn status(did: &str, status: Option<&str>) -> crate::store::Event {
    let at = Value::String(iso(now_millis()));
    match status {
        None => event(
            "#account",
            [
                ("did", Value::String(did.into())),
                ("active", Value::Bool(true)),
                ("time", at),
            ],
        ),
        Some(status) => event(
            "#account",
            [
                ("did", Value::String(did.into())),
                ("active", Value::Bool(false)),
                ("status", Value::String(status.into())),
                ("time", at),
            ],
        ),
    }
}

impl Host {
    /// How `did` was switched off, if it is.
    pub fn deactivation(&self, did: &str) -> Result<Option<Deactivation>, XrpcError> {
        self.store.deactivation(did).map_err(XrpcError::internal)
    }

    /// Refuses `did` if it is switched off.
    pub fn active(&self, did: &str) -> Result<(), XrpcError> {
        match self.deactivation(did)? {
            Some(_) => Err(deactivated(did)),
            None => Ok(()),
        }
    }

    /// Switches `did` off, by `by`, for `stewards` to switch back on:
    /// `false` if it was off.
    pub fn deactivate_account(&self, did: &str, by: &str, stewards: &[String]) -> Result<bool> {
        let events = [status(did, Some("deactivated"))];
        let done = self
            .store
            .deactivate(did, now_millis(), by, stewards, &events)?;
        self.announce();
        Ok(done)
    }

    /// Switches `did` back on: `false` if it was on.
    pub fn activate_account(&self, did: &str) -> Result<bool> {
        let done = self.store.activate(did, &[status(did, None)])?;
        self.announce();
        Ok(done)
    }

    /// Deletes `did` from the host whole, and ends its DID in the
    /// directory, as only this host holds a key to it. Whether it was
    /// ended.
    pub async fn delete_account(&self, did: &str) -> Result<bool> {
        let blobs = self
            .store
            .delete_account(did, &[status(did, Some("deleted"))])?;
        self.remove_blobs(blobs.iter().map(|cid| (did, cid)));
        self.announce();
        let ended = self.tombstone(did).await;
        if let Err(e) = &ended {
            tracing::warn!(did, error = %format!("{e:#}"), "could not end the DID");
        }
        Ok(ended.unwrap_or(false))
    }

    /// Ends `did` in the directory, if nobody but this host could ever use
    /// it again: with a steward's recovery key on it, it is theirs to keep,
    /// to bring the group back elsewhere, and is left alone. Whether it
    /// ended it.
    async fn tombstone(&self, did: &str) -> Result<bool> {
        let last = self.plc.last_op(did).await?;
        let ours = self.secrets.rotation.public().did_key();
        if let atproto::plc::Signed::Update { op, .. } = &last
            && op.rotation_keys.iter().any(|k| *k != ours)
        {
            return Ok(false);
        }
        let end = atproto::plc::tombstone(last.cid(), &self.secrets.rotation);
        self.plc.publish(did, &end).await.context("tombstoning")?;
        Ok(true)
    }
}
