//! The PLC directory, `PLC_URL`: where the host publishes its groups'
//! identity operations, and reads their documents back.

use crate::xrpc::XrpcError;
use anyhow::{Context, Result};
use atproto::plc::Signed;
use axum::http::StatusCode;
use serde_json::Value;

pub struct Directory {
    url: String,
    http: reqwest::Client,
    /// Held from reading a DID's last operation to publishing the next,
    /// so two changes at once never both build on the same one.
    turn: tokio::sync::Mutex<()>,
}

/// An operation the directory answered with an error, and what it said.
#[derive(Debug)]
pub struct Refused {
    pub status: StatusCode,
    pub said: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Refused { status, said } = self;
        write!(f, "the PLC directory refused it ({status}): {said}")
    }
}

impl std::error::Error for Refused {}

/// An operation not published, as its caller is told. One the directory
/// turned away (a 4xx: a key it does not take, too many keys, another
/// change made first) is the caller's to change or send again,
/// `InvalidOperation`; a directory that failed, or never answered, is the
/// host's trouble.
pub fn not_published(e: anyhow::Error) -> XrpcError {
    match e.downcast_ref::<Refused>() {
        Some(r) if r.status.is_client_error() => {
            XrpcError::new(StatusCode::BAD_REQUEST, "InvalidOperation", r.to_string())
        }
        _ => XrpcError::internal(format!("{e:#}")),
    }
}

impl Directory {
    pub fn new(url: &str) -> Directory {
        Directory {
            url: url.trim_end_matches('/').to_string(),
            http: crate::client(),
            turn: Default::default(),
        }
    }

    /// The turn to change a DID's document: held from reading its last
    /// operation until the next is published.
    pub async fn turn(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.turn.lock().await
    }

    /// Publishes `op` as `did`'s next operation.
    pub async fn publish(&self, did: &str, op: &Signed) -> Result<()> {
        let res = self
            .http
            .post(format!("{}/{did}", self.url))
            .json(&op.to_json())
            .send()
            .await?;
        let status = res.status();
        if !status.is_success() {
            let said = res.text().await.unwrap_or_default();
            let refused = Err(Refused { status, said });
            return refused.context(format!("{did}'s operation"));
        }
        Ok(())
    }

    /// `did`'s last operation, as the directory has it.
    pub async fn last_op(&self, did: &str) -> Result<Signed> {
        let res = self
            .http
            .get(format!("{}/{did}/log/last", self.url))
            .send()
            .await?;
        let res = res
            .error_for_status()
            .context(format!("no operations for {did}"))?;
        let op: Value = res.json().await?;
        Ok(Signed::from_json(&op)?)
    }

    /// `did`'s document, as the directory serves it.
    pub async fn document(&self, did: &str) -> Result<Value> {
        let res = self.http.get(format!("{}/{did}", self.url)).send().await?;
        let res = res
            .error_for_status()
            .context(format!("no document for {did}"))?;
        Ok(res.json().await?)
    }
}
