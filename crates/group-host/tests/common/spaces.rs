//! Spaces to test against: an authority elsewhere, a syncer of its
//! space, and reads of a repo in it.

#![allow(dead_code)]

use super::{Running, serve};
use atproto::Cid;
use atproto::plc::Op;
use atproto::space::{
    Commit, SpaceError, accept_signature, check_commit_mac, check_record_mac, encapsulate,
};
use atproto::space_auth::{self, Kind, Make, sign_request};
use atproto::{Curve, Keypair};
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use group_host::repo::Write;
use group_host::time::now_millis;
use serde_json::{Value as Json, json};
use std::sync::{Arc, Mutex};

pub const RIDERS: &str = "riders.groups.test";
pub const RULE: &str = "org.atgrp.rule";

pub fn now() -> u64 {
    now_millis() as u64 / 1000
}

pub fn rule(title: &str) -> Json {
    json!({"$type": RULE, "title": title, "createdAt": "2026-11-03T00:00:00Z"})
}

/// A group space's policy that grants nobody anything.
pub fn closed() -> Json {
    json!({"$type": "org.atgrp.policy", "grants": []})
}

/// A group space's policy that lets anyone read.
pub fn open() -> Json {
    json!({"$type": "org.atgrp.policy", "publicRead": true, "grants": []})
}

pub fn create(collection: &str, rkey: &str, value: Json) -> Write {
    Write::Create {
        collection: collection.into(),
        rkey: rkey.into(),
        value,
    }
}

/// What an authority's PDS was told: each notifyRepoUpdate's (or
/// notifyCredentialsRevoked's) authorization and body.
#[derive(Default)]
pub struct Told {
    pub writes: Mutex<Vec<(String, Json)>>,
    /// What it answers each notification: 200 unless set.
    pub status: Mutex<Option<u16>>,
}

impl Told {
    /// Answers each notification with `status` from now on.
    pub fn answer(&self, status: u16) {
        *self.status.lock().unwrap() = Some(status);
    }
}

pub async fn notify_write(
    State(told): State<Arc<Told>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Json>,
) -> StatusCode {
    let auth = headers["authorization"].to_str().unwrap().to_string();
    told.writes.lock().unwrap().push((auth, body));
    let status = told.status.lock().unwrap().unwrap_or(200);
    StatusCode::from_u16(status).unwrap()
}

/// A space's authority elsewhere: an account on the directory, its PDS
/// here, hearing of writes.
pub struct Authority {
    pub did: String,
    pub key: Keypair,
    pub told: Arc<Told>,
    /// Its rotation key, its PDS, and its last operation, to change its
    /// document with.
    pub rotation: Keypair,
    pub pds: String,
    pub last: Option<Cid>,
}

pub async fn authority(h: &Running, refuse: bool, pds: bool) -> Authority {
    let told = Arc::new(Told::default());
    if refuse {
        told.answer(500);
    }
    let router = Router::new()
        .route(
            "/xrpc/com.atproto.space.notifyRepoUpdate",
            post(notify_write),
        )
        .route(
            "/xrpc/com.atproto.space.notifyCredentialsRevoked",
            post(notify_write),
        )
        .with_state(told.clone());
    let url = serve(router).await;
    let key = Keypair::generate(Curve::K256);
    let rotation = Keypair::generate(Curve::K256);
    let mut op = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        "authority.test",
        &url,
    );
    if !pds {
        op.services.clear();
    }
    let genesis = op.sign(None, &rotation);
    let did = genesis.did();
    let res = h
        .http
        .post(format!("{}/{did}", plc_url(h)))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    Authority {
        did,
        key,
        told,
        rotation,
        pds: url,
        last: Some(genesis.cid()),
    }
}

impl Authority {
    /// Signs with a new key from now on, its document saying so.
    pub async fn rotate(&mut self, h: &Running) {
        self.key = Keypair::generate(Curve::K256);
        let op = Op::atproto(
            &self.key.public().did_key(),
            &[self.rotation.public().did_key()],
            "authority.test",
            &self.pds,
        );
        let signed = op.sign(self.last, &self.rotation);
        let res = h
            .http
            .post(format!("{}/{}", plc_url(h), self.did))
            .json(&signed.to_json())
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success());
        self.last = Some(signed.cid());
    }
}

pub fn plc_url(h: &Running) -> String {
    h.host.config.plc_url.clone()
}

/// A syncer holding a credential to `space`, bound to its key.
pub struct Syncer {
    pub key: Keypair,
    pub credential: String,
}

pub fn credential(authority: &Authority, space: &str, key: &Keypair, iss: &str) -> String {
    let make = Make {
        iss,
        sub: space,
        key_id: Some(&key.public().did_key()),
        ..Make::default()
    };
    space_auth::create(Kind::Credential, &make, &authority.key, now()).unwrap()
}

pub fn syncer(authority: &Authority, space: &str) -> Syncer {
    let key = Keypair::generate(Curve::P256);
    let credential = credential(authority, space, &key, &authority.did);
    Syncer { key, credential }
}

/// What a read answers: its status, headers and body.
pub struct Read {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

impl Read {
    pub fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap()
    }

    pub fn error(&self) -> (u16, String) {
        (self.status, self.json()["error"].as_str().unwrap().into())
    }

    /// The answer's header `name`, if it has one.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|v| v.to_str().unwrap())
    }
}

/// The secret a syncer agreed for one read, and the key share it sent:
/// what it checks the answer's MAC with.
pub struct Mac {
    pub share: String,
    pub secret: [u8; 32],
}

impl Mac {
    /// The commit `read`'s MAC vouches for, if it checks.
    pub fn commit(&self, read: &Read) -> Result<Commit, SpaceError> {
        check_commit_mac(|n| read.header(n), &self.share, &self.secret)
    }

    /// The record (URI, CID) `read`'s MAC vouches for, if it checks.
    pub fn record(&self, read: &Read) -> Result<(String, String), SpaceError> {
        check_record_mac(|n| read.header(n), &self.share, &self.secret)
    }
}

/// `GET nsid?query` with `headers`.
pub async fn read(h: &Running, nsid: &str, query: &str, headers: &[(&str, String)]) -> Read {
    let mut req = h.http.get(format!("{}/xrpc/{nsid}?{query}", h.url));
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let res = req.send().await.unwrap();
    Read {
        status: res.status().as_u16(),
        headers: res.headers().clone(),
        body: res.bytes().await.unwrap().to_vec(),
    }
}

/// The headers a syncer reads `repo` with: its credential, signed, and an
/// `Accept-Signature` asking for a MAC of `profile` (`COMMIT_MAC` or
/// `RECORD_MAC`); and the secret agreed, to check the answer with.
pub fn signed(
    s: &Syncer,
    repo: &str,
    kem: &str,
    profile: &str,
) -> (Vec<(&'static str, String)>, Mac) {
    let authorization = format!("Atproto-Space-Credential {}", s.credential);
    let mut headers: Vec<(&str, String)> = sign_request(&s.key, &authorization, repo)
        .unwrap()
        .into_iter()
        .collect();
    let (share, secret) = encapsulate(kem).unwrap();
    headers.push(("accept-signature", accept_signature(profile, &share)));
    (headers, Mac { share, secret })
}

// A syncing service's notify endpoint, for the host as a space's
// authority to confirm a registration with and tell of changes.

/// What a syncer's notify endpoint heard: each confirmRegistration's body
/// and each notifySpaceUpdate's; and how it answers.
#[derive(Default)]
pub struct Endpoint {
    pub asked: Mutex<Vec<Json>>,
    pub told: Mutex<Vec<Json>>,
    /// What it answers each notifySpaceUpdate: 200 unless set.
    pub status: Mutex<Option<u16>>,
    /// Whether it echoes a challenge other than the one it was asked.
    pub wrong: std::sync::atomic::AtomicBool,
}

impl Endpoint {
    /// Answers each notifySpaceUpdate with `status` from now on.
    pub fn answer(&self, status: u16) {
        *self.status.lock().unwrap() = Some(status);
    }

    /// What it was told, once it was told `times` things.
    pub async fn told(&self, times: usize) -> Vec<Json> {
        for _ in 0..200 {
            let told = self.told.lock().unwrap().clone();
            if told.len() >= times {
                return told;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("not told {times} times");
    }

    /// What it was told of `kind` (an update's `$type`), once it was told
    /// so `times` times: a write's word, forwarded on its own, may land
    /// before or after.
    pub async fn told_of(&self, kind: &str, times: usize) -> Vec<Json> {
        for _ in 0..200 {
            let told = self.told.lock().unwrap().clone();
            let of: Vec<Json> = told
                .into_iter()
                .filter(|t| t["update"]["$type"] == kind)
                .collect();
            if of.len() >= times {
                return of;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("not told of {kind} {times} times");
    }

    /// What it was told, after a while for anything more to arrive.
    pub async fn told_by_now(&self) -> Vec<Json> {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        self.told.lock().unwrap().clone()
    }
}

async fn confirm_registration(
    State(e): State<Arc<Endpoint>>,
    axum::Json(body): axum::Json<Json>,
) -> axum::Json<Json> {
    e.asked.lock().unwrap().push(body.clone());
    let challenge = match e.wrong.load(std::sync::atomic::Ordering::SeqCst) {
        true => json!("not the one asked"),
        false => body["challenge"].clone(),
    };
    axum::Json(json!({ "challenge": challenge }))
}

async fn notify_space_update(
    State(e): State<Arc<Endpoint>>,
    axum::Json(body): axum::Json<Json>,
) -> StatusCode {
    e.told.lock().unwrap().push(body);
    let status = e.status.lock().unwrap().unwrap_or(200);
    StatusCode::from_u16(status).unwrap()
}

/// A syncer's notify endpoint, served: its URL, and what it hears.
pub async fn endpoint() -> (String, Arc<Endpoint>) {
    let e = Arc::new(Endpoint::default());
    let router = Router::new()
        .route(
            "/xrpc/com.atproto.space.confirmRegistration",
            post(confirm_registration),
        )
        .route(
            "/xrpc/com.atproto.space.notifySpaceUpdate",
            post(notify_space_update),
        )
        .with_state(e.clone());
    (serve(router).await, e)
}
