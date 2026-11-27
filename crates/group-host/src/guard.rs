//! What a host that others' data names may be asked. A DID document names
//! whatever PDS and services it likes, a `did:web` whatever host, a handle
//! whatever name, and a client's metadata or a syncer's registration
//! whatever URL, and anyone can make the host read one; so such a host is
//! asked only on https, at public addresses, its connections pinned to the
//! addresses checked, and no redirect is followed ([`pinned_client`]). A
//! host on http itself (`PUBLIC_URL`), as on a network of test servers,
//! asks one on http too, at any address.
//!
//! The PLC directory, the handle resolvers, `DID_WEB_HOSTS`,
//! `DOMAIN_CHECK_AT` and the other services the host is configured with
//! are its settings, not others' data, and are asked as they are set.

use crate::Host;
use atproto::did;
use reqwest::{Client, Url};
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};

/// A client for requests to `url`, an https URL at a public address (or
/// http at any address, with `http_here`, as on a network of test servers
/// whose host is on http itself), and the URL. Its connections go to the
/// addresses checked, so a name that resolves elsewhere a moment later is
/// not followed there, and it follows no redirect.
pub async fn pinned_client(url: &str, http_here: bool) -> Result<(Client, Url), String> {
    let at = Url::parse(url).map_err(|_| format!("{url} is not a URL"))?;
    let scheme_ok = at.scheme() == "https" || (http_here && at.scheme() == "http");
    if !scheme_ok || at.fragment().is_some() || !at.username().is_empty() {
        return Err(format!("{url} is not an https URL"));
    }
    let host = at.host_str().unwrap_or_default().to_string();
    let port = at.port_or_known_default().unwrap_or_default();
    let found = tokio::net::lookup_host(format!("{host}:{port}")).await;
    let addrs: Vec<SocketAddr> = found.map_err(|e| format!("{url}: {e}"))?.collect();
    if !http_here && !addrs.iter().all(|a| public(a.ip())) {
        return Err(format!("{url} is not at a public address"));
    }
    let pinned = crate::client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host.trim_matches(['[', ']']), &addrs)
        .build()
        .expect("a client builds");
    Ok((pinned, at))
}

/// Whether `ip` is on the public internet: not loopback, private,
/// link-local (a cloud's metadata service among them), shared, reserved,
/// documentation or multicast. An IPv6 address is public only as global
/// unicast (`2000::/3`), less documentation's: an address that carries an
/// IPv4 one (NAT64, IPv4-compatible) is not, but as IPv4-mapped, where it
/// is that address.
pub fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 192 && b == 0)
                || (a == 198 && (18..20).contains(&b)))
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => public(IpAddr::V4(v4)),
            None => {
                let [s, t, ..] = v6.segments();
                (s & 0xe000) == 0x2000 && !(s == 0x2001 && t == 0xdb8)
            }
        },
    }
}

impl Host {
    /// A client for requests to `url`, as [`pinned_client`] makes one, and
    /// the URL: http allowed while this host is on http itself.
    pub(crate) async fn pinned(&self, url: &str) -> Result<(Client, Url), String> {
        pinned_client(url, self.config.public_url.starts_with("http://")).await
    }

    /// A client for calling `nsid` at the PDS `doc`, a DID document, names,
    /// as [`Host::pinned`] makes one, and the call's URL: none where it
    /// names no PDS, or one not to be asked.
    pub(crate) async fn at_pds(&self, doc: &Value, nsid: &str) -> Result<(Client, Url), String> {
        let pds = did::pds(doc).ok_or("the document names no PDS")?;
        self.pinned(&format!("{pds}/xrpc/{nsid}")).await
    }
}
