//! Groups to test against: people elsewhere, signed in to this host, and
//! the groups they start.

#![allow(dead_code)]

use super::Running;
use super::spaces::{Authority, authority, now, read};
use atproto::service_auth::{self, Claims};
use serde_json::Value as Json;

pub const NS: &str = "org.atgrp";
/// The host's own methods and views, beside the proposal's.
pub const INTERNAL: &str = "org.atgrp.internal";

/// The NSID of our method `name`: the host's own if it has a lexicon
/// there, else the proposal's.
pub fn nsid(name: &str) -> String {
    let internal = format!("{INTERNAL}.{name}");
    match group_host::lexicons::lexicons().def(&internal) {
        Some(_) => internal,
        None => format!("{NS}.{name}"),
    }
}

/// Service auth from `who` for this host and `lxm`.
pub fn auth(h: &Running, who: &Authority, lxm: &str) -> (&'static str, String) {
    auth_for(who, &h.host.config.service_did, lxm)
}

pub fn auth_for(who: &Authority, aud: &str, lxm: &str) -> (&'static str, String) {
    let claims = Claims {
        iss: who.did.clone(),
        aud: aud.into(),
        exp: now() + 60,
        iat: Some(now()),
        lxm: Some(lxm.into()),
        jti: None,
    };
    (
        "authorization",
        format!("Bearer {}", service_auth::sign(&who.key, &claims)),
    )
}

pub async fn post(h: &Running, nsid: &str, body: &Json, headers: &[(&str, String)]) -> (u16, Json) {
    let mut req = h.http.post(format!("{}/xrpc/{nsid}", h.url)).json(body);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let res = req.send().await.unwrap();
    (
        res.status().as_u16(),
        res.json().await.unwrap_or(Json::Null),
    )
}

pub async fn get(h: &Running, nsid: &str, query: &str, headers: &[(&str, String)]) -> (u16, Json) {
    let got = read(h, nsid, query, headers).await;
    (
        got.status,
        serde_json::from_slice(&got.body).unwrap_or(Json::Null),
    )
}

/// A group started by `founder` with `body`: its DID.
pub async fn start_group(h: &Running, founder: &Authority, body: Json) -> String {
    let lxm = format!("{INTERNAL}.createGroup");
    let (status, made) = post(h, &lxm, &body, &[auth(h, founder, &lxm)]).await;
    assert_eq!(status, 200, "{made}");
    made["did"].as_str().unwrap().to_string()
}

/// Three people elsewhere.
pub async fn people(h: &Running) -> [Authority; 3] {
    [
        authority(h, false, true).await,
        authority(h, false, true).await,
        authority(h, false, true).await,
    ]
}

pub async fn view(h: &Running, group: &str, who: Option<&Authority>) -> Json {
    let lxm = format!("{INTERNAL}.getGroup");
    let headers: Vec<_> = who.map(|w| auth(h, w, &lxm)).into_iter().collect();
    let (status, view) = get(h, &lxm, &format!("group={group}"), &headers).await;
    assert_eq!(status, 200, "{view}");
    view
}

/// Takes `group`'s about record away behind the host's methods, which
/// keep it: a group as one moved here from a host that kept none would
/// arrive.
pub async fn drop_about(h: &Running, group: &str) {
    let write = group_host::repo::Write::Delete {
        collection: format!("{NS}.about"),
        rkey: "self".into(),
    };
    let meta = group_host::group::meta_space(group);
    h.host.space_write(group, &meta, &[write]).await.unwrap();
}
