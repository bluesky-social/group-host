//! What the host is told by its environment: where it answers, who it is,
//! and where its groups' handles and data live. Every variable is in
//! `docs/reference/config.md`.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The port it listens on, `PORT`; 0 for any free one.
    pub port: u16,
    /// Its origin, `PUBLIC_URL`, with no trailing slash.
    pub public_url: String,
    /// Its own DID, `SERVICE_DID`: `did:web:` of the public URL's host
    /// unless set.
    pub service_did: String,
    /// The domain its groups' handles are made under, `HANDLE_DOMAIN`,
    /// with its leading dot: `.groups.example.com`.
    pub handle_domain: String,
    /// The database, blobs and keys, `DATA_DIR`.
    pub data_dir: PathBuf,
    /// The PLC directory groups' identities are published to, `PLC_URL`.
    pub plc_url: String,
}

impl Config {
    /// The config `var` names, as `std::env::var` would read it; the
    /// environment's own with [`Config::from_env`].
    pub fn from(var: impl Fn(&str) -> Option<String>) -> Result<Config> {
        let port = match var("PORT") {
            Some(p) => p.parse().with_context(|| format!("PORT is no port: {p}"))?,
            None => 2600,
        };
        let public_url = var("PUBLIC_URL")
            .unwrap_or_else(|| format!("http://localhost:{port}"))
            .trim_end_matches('/')
            .to_string();
        let Some(host) = public_url
            .strip_prefix("https://")
            .or_else(|| public_url.strip_prefix("http://"))
            .filter(|h| !h.is_empty() && !h.contains('/'))
        else {
            bail!("PUBLIC_URL is no origin: {public_url}");
        };
        let service_did =
            var("SERVICE_DID").unwrap_or_else(|| format!("did:web:{}", host.replace(':', "%3A")));
        let handle_domain = var("HANDLE_DOMAIN").unwrap_or_else(|| ".test".into());
        if !handle_domain.starts_with('.') {
            bail!("HANDLE_DOMAIN starts with a dot: .{handle_domain}");
        }
        let data_dir = var("DATA_DIR").unwrap_or_else(|| "data".into()).into();
        let plc_url = var("PLC_URL").unwrap_or_else(|| "https://plc.directory".into());
        Ok(Config {
            port,
            public_url,
            service_did,
            handle_domain,
            data_dir,
            plc_url,
        })
    }

    pub fn from_env() -> Result<Config> {
        Config::from(|name| std::env::var(name).ok())
    }
}
