//! What the host is told by its environment: where it answers, who it is,
//! and where its groups' handles and data live. Every variable is in
//! `docs/reference/config.md`.

use anyhow::{Context, Result, bail};
use atproto::did;
use std::collections::BTreeMap;
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
    /// Where a `did:web`'s document is read instead of at its own host,
    /// `DID_WEB_HOSTS`, comma-separated `host=address` pairs, for a
    /// network of test servers: `grain.social=127.0.0.1:2584`.
    pub did_web_hosts: BTreeMap<String, String>,
    /// How a handle is resolved, `HANDLE_RESOLVERS`, comma-separated, each
    /// tried in turn: `well-known:<scheme>` for the handle's own
    /// `/.well-known/atproto-did`, or a service answering
    /// `com.atproto.identity.resolveHandle`, which reads its DNS record.
    pub handle_resolvers: Vec<String>,
    /// Where the host reads a DNS TXT record, `DNS_OVER_HTTPS`: a
    /// DNS-over-HTTPS service answering JSON (`application/dns-json`),
    /// for the `_lexicon` record of another app's permission set.
    pub dns_over_https: String,
    /// Who this host starts groups for, `GROUP_FOUNDERS`, comma-separated
    /// DIDs: anyone when it names none.
    pub group_founders: Vec<String>,
    /// The relays asked to read this host's firehose, `PDS_CRAWLERS`,
    /// comma-separated.
    pub crawlers: Vec<String>,
    /// The most bytes one blob may be, `BLOB_UPLOAD_LIMIT`.
    pub blob_upload_limit: u64,
    /// Where the console is, `CONSOLE_URL`, with no trailing slash: where
    /// a member approves an app signing in as their group. The public URL
    /// unless set, as when the console is served beside the host.
    pub console_url: String,
    /// The OAuth clients this host makes groups for with
    /// `provisionGroup`, `PROVISIONING_CLIENTS`, comma-separated client
    /// ids, `*` for any: none unless set.
    pub provisioning_clients: Vec<String>,
    /// Where a handle being typed is looked for, `ACTOR_SEARCH`: `plc`,
    /// the handles in `PLC_URL`'s own export, or the URL of a service
    /// answering `tech.waow.typeahead.searchActors`. The typeahead service
    /// typeahead.waow.tech for plc.directory, which knows the whole
    /// network; `plc` for any other directory, a network of its own.
    pub actor_search: String,
    /// Where someone with no account makes one, `SIGNUP_PDS`, with no
    /// trailing slash: the PDS the console's "Create one" signs them up at,
    /// by its authorization server's sign-up (`prompt=create`).
    pub signup_pds: String,
    /// Who runs this host, `HOST_ADMINS`, comma-separated DIDs: the
    /// accounts that may read its logs and stats. None unless set.
    pub host_admins: Vec<String>,
    /// How many of the lines it logs at info and below it keeps to read
    /// back, `LOGS_KEEP`; any line is kept 30 days at most.
    pub logs_keep: u64,
    /// Where the host asks a group's own domain whether it reaches this
    /// host, before serving the group there, `DOMAIN_CHECK_AT`: a URL
    /// asked in the domain's place, with the domain as its `Host` (a proxy
    /// in front that the host can't reach at its public addresses). Unset,
    /// the domain itself, over https.
    pub domain_check_at: Option<String>,
    /// Whether it limits how much one caller asks, `RATE_LIMITS`: `on`,
    /// unless `off` (`limits`).
    pub rate_limits: bool,
}

/// The service that finds anyone on the network by what is typed.
pub const TYPEAHEAD: &str = "https://typeahead.waow.tech/xrpc/tech.waow.typeahead.searchActors";

/// The DNS-over-HTTPS service the host reads TXT records from unless
/// `DNS_OVER_HTTPS` names another.
pub const DNS_OVER_HTTPS: &str = "https://cloudflare-dns.com/dns-query";

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
        let service_did = match var("SERVICE_DID") {
            Some(did) => did,
            None => did::web(host)?,
        };
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
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect()
        };
        // A service's URL is one with or without its trailing slash; any
        // other item is as written, as a client id ending in one is.
        let urls = |name: &str| -> Vec<String> {
            list(name)
                .into_iter()
                .map(|u| u.trim_end_matches('/').to_string())
                .filter(|u| !u.is_empty())
                .collect()
        };
        let mut did_web_hosts = BTreeMap::new();
        for pair in list("DID_WEB_HOSTS") {
            let Some((host, address)) = pair.split_once('=') else {
                bail!("DID_WEB_HOSTS is host=address pairs, not {pair}");
            };
            did_web_hosts.insert(host.to_string(), address.to_string());
        }
        let group_founders = list("GROUP_FOUNDERS");
        let mut handle_resolvers = urls("HANDLE_RESOLVERS");
        if handle_resolvers.is_empty() {
            handle_resolvers = vec![
                "well-known:https".into(),
                "https://public.api.bsky.app".into(),
            ];
        }
        let crawlers = urls("PDS_CRAWLERS");
        let dns_over_https = var("DNS_OVER_HTTPS").unwrap_or_else(|| DNS_OVER_HTTPS.into());
        if reqwest::Url::parse(&dns_over_https).is_err() {
            bail!("DNS_OVER_HTTPS is no URL: {dns_over_https}");
        }
        let blob_upload_limit = match var("BLOB_UPLOAD_LIMIT") {
            Some(l) => l
                .parse()
                .with_context(|| format!("BLOB_UPLOAD_LIMIT is no number of bytes: {l}"))?,
            None => 50 * 1024 * 1024,
        };
        let provisioning_clients = list("PROVISIONING_CLIENTS");
        let actor_search = match var("ACTOR_SEARCH") {
            Some(s) if s == "plc" || reqwest::Url::parse(&s).is_ok() => s,
            Some(s) => bail!("ACTOR_SEARCH is neither plc nor a URL: {s}"),
            None if plc_url == "https://plc.directory" => TYPEAHEAD.into(),
            None => "plc".into(),
        };
        let console_url = var("CONSOLE_URL").unwrap_or_else(|| public_url.clone());
        let console_url = console_url.trim_end_matches('/').to_string();
        if reqwest::Url::parse(&console_url).is_err() {
            bail!("CONSOLE_URL is no URL: {console_url}");
        }
        let signup_pds = var("SIGNUP_PDS").unwrap_or_else(|| "https://bsky.social".into());
        let signup_pds = signup_pds.trim_end_matches('/').to_string();
        if reqwest::Url::parse(&signup_pds).is_err() {
            bail!("SIGNUP_PDS is no URL: {signup_pds}");
        }
        let host_admins = list("HOST_ADMINS");
        let logs_keep = match var("LOGS_KEEP") {
            Some(n) => n
                .parse()
                .with_context(|| format!("LOGS_KEEP is no number of lines: {n}"))?,
            None => crate::logs::KEEP,
        };
        let domain_check_at = var("DOMAIN_CHECK_AT").map(|u| u.trim_end_matches('/').to_string());
        let rate_limits = match var("RATE_LIMITS").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(other) => bail!("RATE_LIMITS is on or off, not {other}"),
        };
        Ok(Config {
            port,
            public_url,
            service_did,
            handle_domain,
            data_dir,
            plc_url,
            did_web_scheme,
            did_web_hosts,
            handle_resolvers,
            dns_over_https,
            group_founders,
            crawlers,
            blob_upload_limit,
            console_url,
            provisioning_clients,
            actor_search,
            signup_pds,
            host_admins,
            logs_keep,
            domain_check_at,
            rate_limits,
        })
    }

    pub fn from_env() -> Result<Config> {
        Config::from(|name| std::env::var(name).ok())
    }
}
