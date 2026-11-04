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
    /// How a `did:web`'s document is read, `DID_WEB_SCHEME`: `https`, or
    /// `http` for a network of test servers.
    pub did_web_scheme: String,
    /// How a handle is resolved, `HANDLE_RESOLVERS`, comma-separated, each
    /// tried in turn: `well-known:<scheme>` for the handle's own
    /// `/.well-known/atproto-did`, or a service answering
    /// `com.atproto.identity.resolveHandle`, which reads its DNS record.
    pub handle_resolvers: Vec<String>,
    /// Who this host starts groups for, `GROUP_FOUNDERS`, comma-separated
    /// DIDs: anyone when it names none.
    pub group_founders: Vec<String>,
    /// The relays told of this host's news, `PDS_CRAWLERS`, comma-separated.
    pub crawlers: Vec<String>,
    /// The most bytes one blob may be, `BLOB_UPLOAD_LIMIT`.
    pub blob_upload_limit: u64,
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
        let did_web_scheme = var("DID_WEB_SCHEME").unwrap_or_else(|| "https".into());
        if !["https", "http"].contains(&did_web_scheme.as_str()) {
            bail!("DID_WEB_SCHEME is https or http, not {did_web_scheme}");
        }
        let list = |name: &str| -> Vec<String> {
            var(name)
                .unwrap_or_default()
                .split(',')
                .map(|c| c.trim().trim_end_matches('/').to_string())
                .filter(|c| !c.is_empty())
                .collect()
        };
        let group_founders = list("GROUP_FOUNDERS");
        let mut handle_resolvers = list("HANDLE_RESOLVERS");
        if handle_resolvers.is_empty() {
            handle_resolvers = vec![
                "well-known:https".into(),
                "https://public.api.bsky.app".into(),
            ];
        }
        let crawlers = list("PDS_CRAWLERS");
        let blob_upload_limit = match var("BLOB_UPLOAD_LIMIT") {
            Some(l) => l
                .parse()
                .with_context(|| format!("BLOB_UPLOAD_LIMIT is no number of bytes: {l}"))?,
            None => 50 * 1024 * 1024,
        };
        Ok(Config {
            port,
            public_url,
            service_did,
            handle_domain,
            data_dir,
            plc_url,
            did_web_scheme,
            handle_resolvers,
            group_founders,
            crawlers,
            blob_upload_limit,
        })
    }

    pub fn from_env() -> Result<Config> {
        Config::from(|name| std::env::var(name).ok())
    }
}
