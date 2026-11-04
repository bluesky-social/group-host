//! The PLC directory, `PLC_URL`: where the host publishes its groups'
//! identity operations, and reads their documents back.

use anyhow::{Context, Result, bail};
use atproto::plc::Signed;
use serde_json::Value;

pub struct Directory {
    url: String,
    http: reqwest::Client,
}

impl Directory {
    pub fn new(url: &str) -> Directory {
        Directory {
            url: url.trim_end_matches('/').to_string(),
            http: crate::client(),
        }
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
            bail!("the PLC directory refused {did}'s operation ({status}): {said}");
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
