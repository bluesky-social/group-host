//! An account from elsewhere on the network, moved here to become a group.
//! What matters: only the account itself hands its DID over; its records
//! and files come along checked against its old key, and keep their CIDs;
//! it stays switched off until the directory names this host; the
//! operation it sends can only give the account to this host; and whoever
//! moved it runs the group.

mod common;

use atproto::mst::Tree;
use atproto::plc::{Op, Service, Signed, tombstone};
use atproto::service_auth::{self, Claims};
use atproto::space::{Commit as SpaceCommit, LtHash, element, star, write_star};
use atproto::{Cid, Commit, Curve, Keypair, car, drisl};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get as route;
use common::groups::*;
use common::spaces::*;
use common::{Running, serve, start_reachable};
use group_host::group::{members_space, meta_space};
use group_host::import::label_of;
use group_host::import_space::record_key_to_did;
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

const CHECK: &str = "fyi.opensocial.checkImport";
const IMPORT: &str = "fyi.opensocial.importAccount";
const COMPLETE: &str = "fyi.opensocial.completeImport";
const BRING: &str = "fyi.opensocial.importAccountSpace";
const CREATE_ACCOUNT: &str = "com.atproto.server.createAccount";
const AVATAR: &[u8] = &[1, 2, 3, 4];

/// The server an account lives on, and the handles it answers for, as
/// its tests set them.
#[derive(Default)]
struct Old {
    repo: Mutex<Vec<u8>>,
    /// What getRepo and getBlob answer instead of the account's own.
    repo_status: Mutex<Option<u16>>,
    blob: Mutex<Option<Vec<u8>>>,
    names: Mutex<HashMap<String, String>>,
    /// describeServer fails, as a server down does.
    undescribed: Mutex<bool>,
}

async fn old_repo(State(o): State<Arc<Old>>) -> Response {
    match *o.repo_status.lock().unwrap() {
        Some(s) => StatusCode::from_u16(s).unwrap().into_response(),
        None => o.repo.lock().unwrap().clone().into_response(),
    }
}

/// Its one file, in two pages, the second empty.
async fn old_blobs(Query(q): Query<HashMap<String, String>>) -> axum::Json<Json> {
    axum::Json(match q.get("cursor") {
        None => json!({"cids": [Cid::of_raw(AVATAR).to_string()], "cursor": "next"}),
        Some(_) => json!({"cids": [], "cursor": "again"}),
    })
}

async fn old_blob(State(o): State<Arc<Old>>) -> Response {
    match o.blob.lock().unwrap().clone() {
        Some(bytes) => ([("content-type", "image/png")], bytes).into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn describe(State(o): State<Arc<Old>>) -> Response {
    if *o.undescribed.lock().unwrap() {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    axum::Json(json!({"did": "did:web:old.test", "availableUserDomains": [".old.test"]}))
        .into_response()
}

async fn resolve(State(o): State<Arc<Old>>, Query(q): Query<HashMap<String, String>>) -> Response {
    match o.names.lock().unwrap().get(&q["handle"]) {
        Some(did) => axum::Json(json!({ "did": did })).into_response(),
        None => StatusCode::BAD_REQUEST.into_response(),
    }
}

/// An account on its old server, and the host it moves to, which asks
/// that server who a handle names.
struct World {
    h: Running,
    old: Arc<Old>,
    url: String,
    did: String,
    key: Keypair,
    rotation: Keypair,
    /// The profile's CID there, which a like or reply elsewhere points at.
    profile: Cid,
    people: [Authority; 3],
}

/// A record's DRISL, from its JSON.
fn encoded(value: Json) -> Vec<u8> {
    drisl::encode(&drisl::Value::from_json(&value).unwrap())
}

/// `did`'s repo, signed with `key`, of `records` by path.
fn repo_of(did: &str, key: &Keypair, records: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let paths: BTreeMap<String, Cid> = records
        .iter()
        .map(|(p, r)| (p.to_string(), Cid::of_cbor(r)))
        .collect();
    let tree = Tree::build(&paths);
    let commit = Commit::sign(did, tree.root(), "3lz", key);
    let mut blocks: Vec<(Cid, Vec<u8>)> = tree.blocks().into_iter().collect();
    blocks.extend(records.iter().map(|(_, r)| (Cid::of_cbor(r), r.clone())));
    blocks.push((commit.cid(), commit.to_bytes()));
    car::write(
        &[commit.cid()],
        blocks.iter().map(|(c, b)| (c, b.as_slice())),
    )
}

/// An account at `pds` known as `handle`, signing with `key`, its
/// operations with `rotation`: its DID.
async fn plc_account(
    h: &Running,
    key: &Keypair,
    rotation: &Keypair,
    handle: &str,
    pds: &str,
) -> String {
    let op = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        handle,
        pds,
    );
    genesis(h, op, rotation).await
}

/// `op` as a DID's first operation, signed with `rotation`: the DID.
async fn genesis(h: &Running, op: Op, rotation: &Keypair) -> String {
    let genesis = op.sign(None, rotation);
    let did = genesis.did();
    let res = h
        .http
        .post(format!("{}/{did}", plc_url(h)))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    did
}

async fn world_with(handle: &str, vars: &[(&str, &str)]) -> World {
    let old = Arc::new(Old::default());
    let router = Router::new()
        .route("/xrpc/com.atproto.sync.getRepo", route(old_repo))
        .route("/xrpc/com.atproto.sync.listBlobs", route(old_blobs))
        .route("/xrpc/com.atproto.sync.getBlob", route(old_blob))
        .route("/xrpc/com.atproto.server.describeServer", route(describe))
        .route("/xrpc/com.atproto.identity.resolveHandle", route(resolve))
        .with_state(old.clone());
    let url = serve(router).await;
    let mut vars = vars.to_vec();
    vars.push(("HANDLE_RESOLVERS", &url));
    let h = start_reachable(&vars).await;
    let people = people(&h).await;
    let key = Keypair::generate(Curve::K256);
    let rotation = Keypair::generate(Curve::K256);
    let did = plc_account(&h, &key, &rotation, handle, &url).await;
    let avatar = Cid::of_raw(AVATAR).to_string();
    let profile = encoded(json!({
        "$type": "app.bsky.actor.profile",
        "displayName": "Harbor Cycling Club",
        "description": "Rides from the harbor, Saturdays.",
        "avatar": {"$type": "blob", "ref": {"$link": avatar}, "mimeType": "image/png", "size": AVATAR.len()},
    }));
    let records = [
        ("app.bsky.actor.profile/self", profile.clone()),
        (
            "app.bsky.feed.post/3lzpost",
            encoded(
                json!({"$type": "app.bsky.feed.post", "text": "Saturday's ride", "createdAt": "2026-10-01T00:00:00Z"}),
            ),
        ),
        // Not of its collection's type: this host would not keep it.
        (
            "app.bsky.feed.like/3lzlike",
            encoded(
                json!({"$type": "app.bsky.feed.post", "text": "not a like", "createdAt": "2026-10-01T00:00:00Z"}),
            ),
        ),
    ];
    *old.repo.lock().unwrap() = repo_of(&did, &key, &records);
    *old.blob.lock().unwrap() = Some(AVATAR.to_vec());
    World {
        h,
        old,
        url,
        did,
        key,
        rotation,
        profile: Cid::of_cbor(&profile),
        people,
    }
}

async fn world() -> World {
    world_with("harbor.old.test", &[]).await
}

/// `iss`'s token, signed with `key`, for `aud` and `lxm`.
fn token(iss: &str, key: &Keypair, aud: &str, lxm: &str) -> String {
    service_auth::sign(
        key,
        &Claims {
            iss: iss.into(),
            aud: aud.into(),
            exp: now() + 60,
            iat: Some(now()),
            lxm: Some(lxm.into()),
            jti: None,
        },
    )
}

impl World {
    /// The account's own word that it moves here.
    fn proof(&self) -> String {
        let aud = &self.h.host.config.service_did;
        token(&self.did, &self.key, aud, CREATE_ACCOUNT)
    }

    async fn call(&self, who: &Authority, lxm: &str, body: Json) -> (u16, Json) {
        post(&self.h, lxm, &body, &[auth(&self.h, who, lxm)]).await
    }

    async fn start(&self, who: &Authority, extra: Json) -> (u16, Json) {
        let mut body = json!({"serviceJwt": self.proof()});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.call(who, IMPORT, body).await
    }

    async fn check(&self, query: &str) -> (u16, Json) {
        let who = &self.people[0];
        get(&self.h, CHECK, query, &[auth(&self.h, who, CHECK)]).await
    }

    /// What the old server signs, from the credentials importAccount gave,
    /// changed by `change`.
    async fn signed(&self, creds: &Json, change: impl FnOnce(&mut Op)) -> Json {
        let strings = |v: &Json| -> Vec<String> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_string())
                .collect()
        };
        let mut op = Op {
            rotation_keys: strings(&creds["rotationKeys"]),
            verification_methods: creds["verificationMethods"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                .collect(),
            also_known_as: strings(&creds["alsoKnownAs"]),
            services: BTreeMap::from([(
                "atproto_pds".to_string(),
                Service {
                    kind: "AtprotoPersonalDataServer".into(),
                    endpoint: creds["services"]["atproto_pds"]["endpoint"]
                        .as_str()
                        .unwrap()
                        .into(),
                },
            )]),
        };
        change(&mut op);
        let last = self.h.host.plc.last_op(&self.did).await.unwrap();
        op.sign(Some(last.cid()), &self.rotation).to_json()
    }

    /// One of the account's spaces, a repo in one, or a file: `part`.
    async fn bring(&self, who: &Authority, part: &str, bytes: Vec<u8>) -> (u16, Json) {
        let (k, v) = auth(&self.h, who, BRING);
        let res = self
            .h
            .http
            .post(format!(
                "{}/xrpc/{BRING}?did={}&part={part}",
                self.h.url, self.did
            ))
            .header(k, v)
            .header("content-type", "image/png")
            .body(bytes)
            .send()
            .await
            .unwrap();
        (
            res.status().as_u16(),
            res.json().await.unwrap_or(Json::Null),
        )
    }

    /// The account's space of `kind` at `skey`.
    fn space(&self, kind: &str, skey: &str) -> String {
        format!("at://{}/space/{kind}/{skey}", self.did)
    }

    async fn finish(&self, who: &Authority, operation: Json) -> (u16, Json) {
        self.call(
            who,
            COMPLETE,
            json!({"did": self.did, "operation": operation}),
        )
        .await
    }
}

/// A change to an operation, before it is signed.
type Change<'a> = Box<dyn FnOnce(&mut Op) + 'a>;

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// A space meta archive, as an account's server exports one: `policy`,
/// its `state` by key, and its writers by DID, each at `rev`.
fn meta_archive(space: &str, policy: Json, state: &[(&str, Json)], writers: &[&str]) -> Vec<u8> {
    let value = |j: Json| drisl::encode(&drisl::Value::from_json(&j).unwrap());
    let meta =
        drisl::Value::from_json(&json!({"$type": "atproto-space-meta", "ver": 1, "space": space}));
    let drisl::Value::Map(meta) = meta.unwrap() else {
        panic!("a map")
    };
    // Each section in byte order of its keys, as the alpha writes them.
    let state: BTreeMap<Vec<u8>, Vec<u8>> = state
        .iter()
        .map(|(k, v)| (k.as_bytes().to_vec(), value(v.clone())))
        .collect();
    let writer = json!({"rev": "3lzaaaaaaaaab", "data": {"$bytes": "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"}, "spaceRev": "3lzaaaaaaaaac"});
    let writers: BTreeMap<Vec<u8>, Vec<u8>> = writers
        .iter()
        .map(|did| (format!("writer/{did}").into_bytes(), value(writer.clone())))
        .collect();
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = vec![(b"policy".to_vec(), value(policy))];
    pairs.extend(state);
    pairs.extend(writers);
    write_star(
        &meta,
        pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())),
    )
}

fn member(did: &str, read: bool, write: bool) -> (String, Json) {
    (
        format!("com.atproto.memberspace.member/{did}"),
        json!({"$type": "com.atproto.memberspace.member", "read": read, "write": write}),
    )
}

fn memberspace(public_read: bool) -> Json {
    json!({"$type": "com.atproto.memberspace.policy", "publicRead": public_read})
}

/// `did`'s repo in `space` as a STAR archive, of `records` by path: its
/// commit's digest theirs, or `data` in its place.
fn space_repo(did: &str, space: &str, records: &[(&str, Json)], data: Option<[u8; 32]>) -> Vec<u8> {
    let records: BTreeMap<String, Vec<u8>> = records
        .iter()
        .map(|(p, r)| (p.to_string(), encoded(r.clone())))
        .collect();
    let mut hash = LtHash::default();
    // A path that is no record's, which the host refuses, is no element.
    for (path, value) in &records {
        if let Some((collection, rkey)) = path.split_once('/') {
            hash.add(&element(collection, rkey, &Cid::of_cbor(value)));
        }
    }
    let commit = SpaceCommit {
        repo: did.into(),
        space: space.into(),
        rev: "3lzaaaaaaaaad".into(),
        data: data.unwrap_or(hash.digest()),
    };
    star(&commit, &records)
}

fn note(text: &str) -> Json {
    json!({"$type": "com.example.note", "text": text, "createdAt": "2026-10-01T00:00:00Z"})
}

#[tokio::test]
async fn copies_its_repo_and_files_switched_off_and_says_what_the_directory_should_name() {
    let w = world().await;
    let (status, out) = w.start(&w.people[0], json!({})).await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(out["did"], w.did.as_str());
    // Its handle under its old server's domain does not come along.
    assert_eq!(out["handle"], "harbor.groups.test");
    assert_eq!(out["address"], "harbor.groups.test");
    assert_eq!(out["displayName"], "Harbor Cycling Club");
    assert_eq!(out["records"], 2);
    assert_eq!(out["blobs"], 1);
    let creds = &out["recommendedCreds"];
    assert_eq!(creds["alsoKnownAs"], json!(["at://harbor.groups.test"]));
    assert_eq!(
        creds["services"]["atproto_pds"]["endpoint"],
        w.h.url.as_str()
    );
    let host = &w.h.host;
    assert!(host.deactivation(&w.did).unwrap().is_some());
    // The same records, so what points at them still does.
    let profile = host.store.record(&w.did, "app.bsky.actor.profile", "self");
    assert_eq!(profile.unwrap().unwrap().cid, w.profile);
    assert!(
        host.store
            .record(&w.did, "app.bsky.feed.like", "3lzlike")
            .unwrap()
            .is_none()
    );
    let avatar = Cid::of_raw(AVATAR);
    assert_eq!(host.read_blob(&w.did, &avatar).unwrap().unwrap().1, AVATAR);
    // No group yet.
    assert!(
        host.group_record(
            &w.did,
            &meta_space(&w.did),
            "fyi.opensocial.profile",
            "self"
        )
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn keeps_a_handle_on_a_domain_of_its_own_that_still_names_it() {
    let w = world_with("harborcycling.org", &[]).await;
    w.old
        .names
        .lock()
        .unwrap()
        .insert("harborcycling.org".into(), w.did.clone());
    let (status, out) = w.start(&w.people[0], json!({})).await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(out["handle"], "harborcycling.org");
    assert_eq!(out["address"], "harborcycling.groups.test");
    let creds = out["recommendedCreds"].clone();
    let done = w.finish(&w.people[0], w.signed(&creds, |_| {}).await).await;
    assert_eq!(done.0, 200, "{}", done.1);
    assert_eq!(done.1["handle"], "harborcycling.org");
    assert_eq!(done.1["address"], "harborcycling.groups.test");
    let account = w.h.host.store.account(&w.did).unwrap().unwrap();
    assert_eq!(account.handle, "harborcycling.org");
}

#[tokio::test]
async fn keeps_no_handle_of_a_server_that_cannot_say_whose_it_is() {
    // A handle under the old server's domain still names the account, but
    // the server cannot say it gives such handles out: it is not kept.
    let w = world().await;
    w.old
        .names
        .lock()
        .unwrap()
        .insert("harbor.old.test".into(), w.did.clone());
    *w.old.undescribed.lock().unwrap() = true;
    let (status, said) = w.check(&format!("did={}", w.did)).await;
    assert_eq!(status, 200, "{said}");
    assert_eq!(said["keeps"], false, "{said}");
    assert_eq!(said["handle"], "harbor.groups.test", "{said}");
}

#[tokio::test]
async fn takes_an_address_asked_for() {
    let w = world().await;
    let (status, out) = w
        .start(&w.people[0], json!({"address": "Harbor Riders"}))
        .await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(out["address"], "harbor-riders.groups.test");
}

#[tokio::test]
async fn takes_only_the_accounts_own_proof_for_this_host_and_create_account() {
    let w = world().await;
    let dana = &w.people[0];
    let ours = w.h.host.config.service_did.clone();
    let other = Keypair::generate(Curve::K256);
    for (jwt, why) in [
        (token(&w.did, &other, &ours, CREATE_ACCOUNT), "another key"),
        (
            token(&w.did, &w.key, &ours, "com.atproto.repo.createRecord"),
            "another method",
        ),
        (
            token(&w.did, &w.key, "did:web:other.test", CREATE_ACCOUNT),
            "another host",
        ),
        ("not.a.token".to_string(), "no token"),
    ] {
        let said = w.call(dana, IMPORT, json!({ "serviceJwt": jwt })).await;
        assert_eq!(error(&said), (400, "InvalidToken"), "{why}");
    }
    // For this host's service, as an OAuth app's rpc scope names it.
    let service = format!("{ours}#group_host");
    let said = w
        .call(
            dana,
            IMPORT,
            json!({"serviceJwt": token(&w.did, &w.key, &service, CREATE_ACCOUNT)}),
        )
        .await;
    assert_eq!(said.0, 200, "{}", said.1);
    assert_eq!(
        error(&w.call(dana, IMPORT, json!({})).await),
        (400, "InvalidRequest")
    );
}

#[tokio::test]
async fn refuses_the_account_signed_in_a_did_web_and_one_whose_did_names_no_key() {
    let w = world().await;
    let itself = Authority {
        did: w.did.clone(),
        key: w.key.clone(),
        told: Arc::default(),
        rotation: w.rotation.clone(),
        pds: w.url.clone(),
        last: None,
    };
    assert_eq!(
        error(&w.start(&itself, json!({})).await),
        (400, "SameAccount")
    );
    let web = token(
        "did:web:club.example",
        &w.key,
        &w.h.host.config.service_did,
        CREATE_ACCOUNT,
    );
    let said = w
        .call(&w.people[0], IMPORT, json!({ "serviceJwt": web }))
        .await;
    assert_eq!(error(&said), (400, "NotPlc"));
    let said = w
        .call(
            &w.people[0],
            IMPORT,
            json!({"serviceJwt": token("did:plc:nobody", &w.key, &w.h.host.config.service_did, CREATE_ACCOUNT)}),
        )
        .await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    // A DID whose document names no key.
    let rotation = Keypair::generate(Curve::K256);
    let mut op = Op::atproto(
        "did:key:z",
        &[rotation.public().did_key()],
        "x.old.test",
        &w.url,
    );
    op.verification_methods.clear();
    let keyless = genesis(&w.h, op, &rotation).await;
    let jwt = token(
        &keyless,
        &w.key,
        &w.h.host.config.service_did,
        CREATE_ACCOUNT,
    );
    let said = w
        .call(&w.people[0], IMPORT, json!({ "serviceJwt": jwt }))
        .await;
    assert_eq!(error(&said), (400, "InvalidToken"));
    let said = w.check(&format!("did={keyless}")).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

#[tokio::test]
async fn moves_accounts_only_for_whom_the_host_starts_groups() {
    let w = world_with("harbor.old.test", &[("GROUP_FOUNDERS", "did:plc:someone")]).await;
    assert_eq!(
        error(&w.start(&w.people[0], json!({})).await),
        (403, "NotAllowed")
    );
}

#[tokio::test]
async fn starts_over_for_whoever_started_and_for_nobody_else() {
    let w = world().await;
    let [dana, bob, _] = &w.people;
    assert_eq!(w.start(dana, json!({})).await.0, 200);
    let (status, again) = w.start(dana, json!({})).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["records"], 2);
    assert_eq!(error(&w.start(bob, json!({})).await), (400, "AlreadyHere"));
    // Nor an account that lives here already.
    let group = start_group(
        &w.h,
        dana,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let signing = w.h.host.store.account(&group).unwrap().unwrap().signing;
    let jwt = token(
        &group,
        &signing,
        &w.h.host.config.service_did,
        CREATE_ACCOUNT,
    );
    let said = w.call(bob, IMPORT, json!({ "serviceJwt": jwt })).await;
    assert_eq!(error(&said), (400, "AlreadyHere"));
}

#[tokio::test]
async fn leaves_nothing_behind_when_the_copy_fails() {
    let w = world().await;
    let dana = &w.people[0];
    // A repo another key signed.
    let other = Keypair::generate(Curve::K256);
    let theirs = repo_of(
        &w.did,
        &other,
        &[("a.b.c/r", encoded(json!({"$type": "a.b.c"})))],
    );
    *w.old.repo.lock().unwrap() = theirs;
    assert_eq!(error(&w.start(dana, json!({})).await), (400, "InvalidRepo"));
    assert!(w.h.host.store.account(&w.did).unwrap().is_none());
    assert!(
        w.h.host
            .read_blob(&w.did, &Cid::of_raw(AVATAR))
            .unwrap()
            .is_none()
    );
    // Its server refusing its repo.
    *w.old.repo_status.lock().unwrap() = Some(404);
    assert_eq!(
        error(&w.start(dana, json!({})).await),
        (502, "UpstreamFailure")
    );
    assert!(w.h.host.store.account(&w.did).unwrap().is_none());
}

#[tokio::test]
async fn says_when_the_old_server_cannot_be_reached() {
    let w = world().await;
    let key = Keypair::generate(Curve::K256);
    let rotation = Keypair::generate(Curve::K256);
    let gone = plc_account(&w.h, &key, &rotation, "gone.old.test", "http://127.0.0.1:1").await;
    let jwt = token(&gone, &key, &w.h.host.config.service_did, CREATE_ACCOUNT);
    let said = w
        .call(&w.people[0], IMPORT, json!({ "serviceJwt": jwt }))
        .await;
    assert_eq!(error(&said), (502, "UpstreamFailure"), "{}", said.1);
    let nowhere = plc_account(&w.h, &key, &rotation, "nowhere.old.test", "not a url").await;
    let jwt = token(&nowhere, &key, &w.h.host.config.service_did, CREATE_ACCOUNT);
    let said = w
        .call(&w.people[0], IMPORT, json!({ "serviceJwt": jwt }))
        .await;
    assert_eq!(error(&said), (502, "UpstreamFailure"), "{}", said.1);
}

#[tokio::test]
async fn goes_on_without_a_file_that_does_not_come() {
    let w = world().await;
    *w.old.blob.lock().unwrap() = None;
    let (status, out) = w.start(&w.people[0], json!({})).await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(
        (out["records"].clone(), out["blobs"].clone()),
        (json!(2), json!(0))
    );
    // Its profile still points at it, as at a file deleted there.
    let profile =
        w.h.host
            .store
            .record(&w.did, "app.bsky.actor.profile", "self");
    assert_eq!(profile.unwrap().unwrap().cid, w.profile);
    // Nor one that is not what it says.
    *w.old.blob.lock().unwrap() = Some(b"something else".to_vec());
    let (status, out) = w.start(&w.people[0], json!({})).await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(out["blobs"], 0);
}

#[tokio::test]
async fn takes_its_address_for_a_name_when_its_profile_gives_none() {
    let w = world().await;
    let post = encoded(
        json!({"$type": "app.bsky.feed.post", "text": "hi", "createdAt": "2026-10-01T00:00:00Z"}),
    );
    *w.old.repo.lock().unwrap() = repo_of(&w.did, &w.key, &[("app.bsky.feed.post/3lzpost", post)]);
    let (status, out) = w.start(&w.people[0], json!({})).await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(
        (out["records"].clone(), out["displayName"].clone()),
        (json!(1), json!("harbor"))
    );
    // Nothing it holds this host would keep: an empty repo.
    let like = encoded(json!({"$type": "app.bsky.feed.post", "text": "not a like"}));
    *w.old.repo.lock().unwrap() = repo_of(&w.did, &w.key, &[("app.bsky.feed.like/3lzlike", like)]);
    let (status, out) = w.start(&w.people[0], json!({})).await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(out["records"], 0);
}

#[tokio::test]
async fn sends_the_operation_switches_it_on_and_makes_a_group_its_mover_runs() {
    let w = world().await;
    let dana = &w.people[0];
    let (_, out) = w.start(dana, json!({})).await;
    let operation = w.signed(&out["recommendedCreds"], |_| {}).await;
    let (status, done) = w.finish(dana, operation.clone()).await;
    assert_eq!(status, 200, "{done}");
    let host = &w.h.host;
    let last = host.plc.last_op(&w.did).await.unwrap();
    assert_eq!(last.to_json(), operation);
    assert!(host.deactivation(&w.did).unwrap().is_none());
    assert_eq!(done["handle"], "harbor.groups.test");
    assert_eq!(done["address"], "harbor.groups.test");
    assert_eq!(done["meta"], meta_space(&w.did).as_str());
    assert_eq!(done["members"], members_space(&w.did).as_str());
    let profile = host
        .group_record(
            &w.did,
            &meta_space(&w.did),
            "fyi.opensocial.profile",
            "self",
        )
        .unwrap()
        .unwrap();
    assert_eq!(profile["displayName"], "Harbor Cycling Club");
    assert_eq!(profile["description"], "Rides from the harbor, Saturdays.");
    assert!(
        host.roles(&w.did, &dana.did)
            .unwrap()
            .contains(&"admin".to_string())
    );
    // Its posts stay where they were in its repo.
    assert!(
        host.store
            .record(&w.did, "app.bsky.feed.post", "3lzpost")
            .unwrap()
            .is_some()
    );
    assert!(host.store.pending_import(&w.did).unwrap().is_none());
    let lxm = format!("{NS}.listGroupTokens");
    let (_, grants) = get(
        &w.h,
        &lxm,
        &format!("group={}", w.did),
        &[auth(&w.h, dana, &lxm)],
    )
    .await;
    assert_eq!(
        grants["activity"][0]["action"],
        "moved the account here as a group"
    );
}

#[tokio::test]
async fn takes_an_operation_the_directory_has_already() {
    let w = world().await;
    let dana = &w.people[0];
    let (_, out) = w.start(dana, json!({})).await;
    let operation = w.signed(&out["recommendedCreds"], |_| {}).await;
    // Its server sent it itself, as submitPlcOperation does.
    let signed = Signed::from_json(&operation).unwrap();
    w.h.host.plc.publish(&w.did, &signed).await.unwrap();
    let (status, done) = w.finish(dana, operation).await;
    assert_eq!(status, 200, "{done}");
    assert!(w.h.host.deactivation(&w.did).unwrap().is_none());
}

#[tokio::test]
async fn refuses_an_operation_that_does_not_give_the_account_to_this_host() {
    let w = world().await;
    let dana = &w.people[0];
    let (_, out) = w.start(dana, json!({})).await;
    let creds = &out["recommendedCreds"];
    let elsewhere = Keypair::generate(Curve::K256).public().did_key();
    let changes: Vec<Change> = vec![
        Box::new(|op| {
            op.services.get_mut("atproto_pds").unwrap().endpoint = "https://evil.test".into()
        }),
        Box::new(|op| op.rotation_keys = vec![elsewhere.clone()]),
        Box::new(|op| {
            op.verification_methods
                .insert("atproto".into(), elsewhere.clone());
        }),
        Box::new(|op| {
            op.verification_methods
                .insert("atproto_kem".into(), elsewhere.clone());
        }),
        Box::new(|op| op.also_known_as = vec!["at://someone.else".into()]),
    ];
    for change in changes {
        let operation = w.signed(creds, change).await;
        assert_eq!(
            error(&w.finish(dana, operation).await),
            (400, "InvalidOperation")
        );
    }
    let last = w.h.host.plc.last_op(&w.did).await.unwrap();
    let ended = tombstone(last.cid(), &w.rotation).to_json();
    assert_eq!(
        error(&w.finish(dana, ended).await),
        (400, "InvalidOperation")
    );
    assert_eq!(
        error(&w.finish(dana, json!("not an operation")).await),
        (400, "InvalidRequest")
    );
    // One the directory refuses: not signed by a key it holds.
    let mut wrong = w.signed(creds, |_| {}).await;
    let theirs = Signed::from_json(&wrong).unwrap();
    let Signed::Update { op, prev, .. } = theirs else {
        panic!()
    };
    wrong = op.sign(prev, &Keypair::generate(Curve::K256)).to_json();
    assert_eq!(
        error(&w.finish(dana, wrong).await),
        (400, "InvalidOperation")
    );
    assert!(w.h.host.deactivation(&w.did).unwrap().is_some());
}

#[tokio::test]
async fn finishes_only_for_whoever_started_the_move() {
    let w = world().await;
    let [dana, bob, _] = &w.people;
    let (_, out) = w.start(dana, json!({})).await;
    let operation = w.signed(&out["recommendedCreds"], |_| {}).await;
    assert_eq!(error(&w.finish(bob, operation).await), (400, "NotFound"));
    assert_eq!(
        error(&w.call(dana, COMPLETE, json!({})).await),
        (400, "InvalidRequest")
    );
}

#[tokio::test]
async fn checks_what_a_move_would_make_of_it_before_anything_moves() {
    let w = world().await;
    let (status, said) = w.check(&format!("did={}", w.did)).await;
    assert_eq!(status, 200, "{said}");
    assert_eq!(said["handle"], "harbor.groups.test");
    assert_eq!(said["address"], "harbor.groups.test");
    assert_eq!(said["label"], "harbor");
    assert_eq!(said["keeps"], false);
    assert_eq!(said["from"], "harbor.old.test");
    assert!(said.get("refused").is_none());
    assert_eq!(said["pds"], w.url.as_str());
    assert!(said.get("recommendedCreds").is_none());
    assert!(w.h.host.store.account(&w.did).unwrap().is_none());
    let (_, said) = w.check(&format!("did={}&address=Riders", w.did)).await;
    assert_eq!(said["address"], "riders.groups.test");
    let (_, said) = w.check(&format!("did={}&address=-", w.did)).await;
    assert_eq!(said["refused"]["error"], "InvalidHandle");
    // As importAccount would refuse it.
    let said = w.start(&w.people[0], json!({"address": "-"})).await;
    assert_eq!(error(&said), (400, "InvalidHandle"));
    // An earlier move of the same account holds nothing that counts.
    let (status, started) = w.start(&w.people[0], json!({})).await;
    assert_eq!(status, 200, "{started}");
    let (_, said) = w.check(&format!("did={}", w.did)).await;
    assert!(said.get("refused").is_none(), "{said}");
    // Whoever is moving it is told again what its operation should name;
    // nobody else is.
    assert_eq!(said["recommendedCreds"], started["recommendedCreds"]);
    let other = &w.people[1];
    let (_, said) = get(
        &w.h,
        CHECK,
        &format!("did={}", w.did),
        &[auth(&w.h, other, CHECK)],
    )
    .await;
    assert!(said.get("recommendedCreds").is_none(), "{said}");
    // One known by no handle takes the address asked for, and only that.
    let rotation = Keypair::generate(Curve::K256);
    let mut op = Op::atproto(
        &w.key.public().did_key(),
        &[rotation.public().did_key()],
        "x",
        &w.url,
    );
    op.also_known_as.clear();
    let unnamed = genesis(&w.h, op, &rotation).await;
    let (_, said) = w.check(&format!("did={unnamed}&address=club")).await;
    assert_eq!(said["handle"], "club.groups.test");
    assert!(said.get("from").is_none(), "{said}");
    let (_, said) = w.check(&format!("did={unnamed}")).await;
    assert_eq!(said["refused"]["error"], "InvalidHandle");
    // A handle under this host's domain, which this host gives out, does
    // not come along as its own.
    let rotation = Keypair::generate(Curve::K256);
    let op = Op::atproto(
        &w.key.public().did_key(),
        &[rotation.public().did_key()],
        "club.groups.test",
        &w.url,
    );
    let ours = genesis(&w.h, op, &rotation).await;
    w.old
        .names
        .lock()
        .unwrap()
        .insert("club.groups.test".into(), ours.clone());
    let (_, said) = w.check(&format!("did={ours}")).await;
    assert_eq!(
        (said["keeps"].clone(), said["handle"].clone()),
        (json!(false), json!("club.groups.test"))
    );
    let said = w.check("did=did:plc:nobody").await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    let said = w.check("did=did:web:club.example").await;
    assert_eq!(error(&said), (400, "NotPlc"));
    let said = get(&w.h, CHECK, &format!("did={}", w.did), &[]).await;
    assert_eq!(said.0, 401);
}

#[tokio::test]
async fn says_when_its_address_or_its_own_domain_is_taken_here() {
    let w = world_with("harborcycling.org", &[]).await;
    let dana = &w.people[0];
    // Riders took another handle, and keeps its address.
    let riders = start_group(
        &w.h,
        dana,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    w.h.host
        .store
        .set_handle(&riders, "harborcycling.org", &[])
        .unwrap();
    let (_, said) = w.check(&format!("did={}&address=riders", w.did)).await;
    assert_eq!(said["refused"]["error"], "HandleNotAvailable");
    let said = w.start(dana, json!({"address": "riders"})).await;
    assert_eq!(error(&said), (400, "HandleNotAvailable"));
    // And the domain it would keep, which Riders has now.
    w.old
        .names
        .lock()
        .unwrap()
        .insert("harborcycling.org".into(), w.did.clone());
    let (_, said) = w.check(&format!("did={}", w.did)).await;
    assert_eq!(said["keeps"], true);
    assert_eq!(said["refused"]["error"], "HandleNotAvailable");
}

#[tokio::test]
async fn brings_its_spaces_and_tells_them_once_it_is_switched_on() {
    let w = world().await;
    let [dana, alice, carol] = &w.people;
    let (status, out) = w.start(dana, json!({})).await;
    assert_eq!(status, 200, "{out}");
    let host = &w.h.host;

    // A forum it governs: its members, who reads it, and who writes there.
    let forum = w.space("com.example.forum", "general");
    let local = "did:web:localhost%3A2583";
    let (alice_key, alice_member) = member(&alice.did, true, true);
    let (local_key, local_member) = member("did:web:localhost:2583", true, false);
    let archive = meta_archive(
        &forum,
        memberspace(true),
        &[(&alice_key, alice_member), (&local_key, local_member)],
        &[&alice.did, &w.did],
    );
    let said = w.bring(dana, "meta", archive.clone()).await;
    assert_eq!(said, (200, json!({ "space": forum })));
    let kept = host.store.space(&forum).unwrap().unwrap();
    assert!(kept.public_read);
    assert_eq!(kept.authority, w.did);
    let granted = |space: &str, who: &str| host.store.granted(space, who).unwrap();
    assert_eq!(granted(&forum, &alice.did), Some((true, true)));
    assert_eq!(granted(&forum, local), Some((true, false)));
    // Its own repo there comes as a repo, and the space hears of it later.
    let writers = |space: &str| -> Vec<String> {
        let all = host.store.writers(space, None, 100).unwrap();
        all.into_iter().map(|w| w.did).collect()
    };
    assert_eq!(writers(&forum), vec![alice.did.clone()]);

    // A file in it, then its repo there, holding that file and one that
    // never came; a record not of its collection's type is left out.
    const PHOTO: &[u8] = &[9, 8, 7];
    let photo = Cid::of_raw(PHOTO).to_string();
    let said = w.bring(dana, "blob", PHOTO.to_vec()).await;
    assert_eq!(said, (200, json!({ "cid": photo })));
    let blob = |cid: &str| json!({"$type": "blob", "ref": {"$link": cid}, "mimeType": "image/png", "size": 3});
    let mut with_photo = note("the harbor at dawn");
    with_photo["image"] = blob(&photo);
    let mut lost = note("a file deleted there");
    lost["image"] = blob(&Cid::of_raw(b"gone").to_string());
    let records = [
        ("com.example.note/1", with_photo),
        ("com.example.note/2", lost),
        ("com.example.like/3", note("not a like")),
    ];
    let said = w
        .bring(dana, "repo", space_repo(&w.did, &forum, &records, None))
        .await;
    assert_eq!(said, (200, json!({ "space": forum, "records": 2 })));
    let paths = host.store.space_paths(&w.did, &forum).unwrap();
    assert_eq!(
        paths.keys().collect::<Vec<_>>(),
        ["com.example.note/1", "com.example.note/2"]
    );
    assert_eq!(
        host.read_blob(&w.did, &Cid::parse(&photo).unwrap())
            .unwrap()
            .unwrap()
            .1,
        PHOTO
    );
    assert_eq!(writers(&forum), vec![alice.did.clone()]);

    // A space only it uses.
    let diary = w.space("com.example.diary", "self");
    let personal = json!({"$type": "com.atproto.personalspace.policy"});
    let said = w
        .bring(dana, "meta", meta_archive(&diary, personal, &[], &[]))
        .await;
    assert_eq!(said.0, 200, "{}", said.1);
    assert!(!host.store.space(&diary).unwrap().unwrap().public_read);

    // Its repo in a space another group here governs, which lets it write.
    let elsewhere = start_group(
        &w.h,
        carol,
        json!({"handle": "lighthouse.groups.test", "displayName": "Lighthouse"}),
    )
    .await;
    let calendar = host
        .create_space(&elsewhere, "com.example.calendar", "main", false)
        .unwrap();
    host.grant(&calendar, &w.did, true, true).unwrap();
    let event = [("com.example.note/1", note("a ride on Saturday"))];
    let said = w
        .bring(dana, "repo", space_repo(&w.did, &calendar, &event, None))
        .await;
    assert_eq!(said, (200, json!({ "space": calendar, "records": 1 })));
    assert!(writers(&calendar).is_empty());
    // And in one that lets it write no more: its authority turns it away.
    let closed = host
        .create_space(&elsewhere, "com.example.calendar", "old", false)
        .unwrap();
    let said = w
        .bring(dana, "repo", space_repo(&w.did, &closed, &event, None))
        .await;
    assert_eq!(said.0, 200, "{}", said.1);

    // Switched on here, it tells each space of its repo there, by its key
    // here; the spaces it governs keep its group's own beside them.
    let operation = w.signed(&out["recommendedCreds"], |_| {}).await;
    let (status, done) = w.finish(dana, operation).await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(writers(&forum), vec![alice.did.clone(), w.did.clone()]);
    assert_eq!(writers(&calendar), vec![w.did.clone()]);
    assert!(writers(&closed).is_empty());
    assert!(host.store.space(&meta_space(&w.did)).unwrap().is_some());
}

#[tokio::test]
async fn brings_a_space_again_in_place_of_what_came_before() {
    let w = world().await;
    let [dana, alice, bob] = &w.people;
    w.start(dana, json!({})).await;
    let host = &w.h.host;
    let forum = w.space("com.example.forum", "general");
    let (alice_key, alice_member) = member(&alice.did, true, true);
    let first = meta_archive(
        &forum,
        memberspace(true),
        &[(&alice_key, alice_member)],
        &[&alice.did],
    );
    assert_eq!(w.bring(dana, "meta", first).await.0, 200);
    let (bob_key, bob_member) = member(&bob.did, true, false);
    let again = meta_archive(
        &forum,
        memberspace(false),
        &[(&bob_key, bob_member)],
        &[&bob.did],
    );
    assert_eq!(w.bring(dana, "meta", again).await.0, 200);
    assert!(!host.store.space(&forum).unwrap().unwrap().public_read);
    assert_eq!(host.store.granted(&forum, &alice.did).unwrap(), None);
    assert_eq!(
        host.store.granted(&forum, &bob.did).unwrap(),
        Some((true, false))
    );
    let writers = host.store.writers(&forum, None, 100).unwrap();
    assert_eq!(
        writers.iter().map(|w| w.did.as_str()).collect::<Vec<_>>(),
        [bob.did.as_str()]
    );

    let two = [
        ("com.example.note/1", note("one")),
        ("com.example.note/2", note("two")),
    ];
    let said = w
        .bring(dana, "repo", space_repo(&w.did, &forum, &two, None))
        .await;
    assert_eq!(said.0, 200, "{}", said.1);
    let one = [("com.example.note/2", note("two, again"))];
    let said = w
        .bring(dana, "repo", space_repo(&w.did, &forum, &one, None))
        .await;
    assert_eq!(said, (200, json!({ "space": forum, "records": 1 })));
    let paths = host.store.space_paths(&w.did, &forum).unwrap();
    assert_eq!(paths.keys().collect::<Vec<_>>(), ["com.example.note/2"]);
    assert_eq!(
        paths["com.example.note/2"],
        Cid::of_cbor(&encoded(note("two, again")))
    );

    // Started over, what it brought goes with the rest.
    assert_eq!(w.start(dana, json!({})).await.0, 200);
    assert!(host.store.space(&forum).unwrap().is_none());
    assert!(host.store.space_paths(&w.did, &forum).unwrap().is_empty());
}

#[tokio::test]
async fn brings_only_what_this_host_would_keep_as_its_policy_says() {
    let w = world().await;
    let [dana, alice, _] = &w.people;
    let forum = w.space("com.example.forum", "general");
    let refused = async |part: &str, bytes: Vec<u8>| {
        let said = w.bring(dana, part, bytes).await;
        (
            said.0,
            said.1["error"].as_str().unwrap_or_default().to_string(),
        )
    };
    let personal = json!({"$type": "com.atproto.personalspace.policy"});
    let archive = meta_archive(&forum, personal.clone(), &[], &[]);
    // Nothing moving here, or not the caller's move.
    assert_eq!(
        refused("meta", archive.clone()).await,
        (400, "NotFound".into())
    );
    w.start(dana, json!({})).await;
    assert_eq!(
        error(&w.bring(alice, "meta", archive.clone()).await),
        (400, "NotFound")
    );
    assert_eq!(
        refused("star", archive).await,
        (400, "InvalidRequest".into())
    );

    let is = |code: &str| (400, code.to_string());
    let (alice_key, alice_member) = member(&alice.did, true, true);
    let others = format!("at://{}/space/com.example.forum/general", alice.did);
    for (archive, why) in [
        (b"not an archive".to_vec(), "InvalidArchive"),
        (
            meta_archive(&others, personal.clone(), &[], &[]),
            "NotSpaceOwner",
        ),
        (
            meta_archive(&meta_space(&w.did), personal.clone(), &[], &[]),
            "SpaceReserved",
        ),
        (
            meta_archive(&members_space(&w.did), personal.clone(), &[], &[]),
            "SpaceReserved",
        ),
        (
            meta_archive(
                &forum,
                json!({"$type": "com.atproto.managedspace.policy", "managingApp": "did:web:app.test"}),
                &[],
                &[],
            ),
            "UnsupportedPolicy",
        ),
        (
            meta_archive(&forum, json!({"publicRead": true}), &[], &[]),
            "UnsupportedPolicy",
        ),
        (
            meta_archive(
                &forum,
                json!({"$type": "com.atproto.memberspace.policy", "publicWrite": true}),
                &[],
                &[],
            ),
            "UnsupportedPolicy",
        ),
        (
            meta_archive(
                &forum,
                json!({"$type": "com.atproto.memberspace.policy", "allowedApps": []}),
                &[],
                &[],
            ),
            "UnsupportedPolicy",
        ),
        // Members are a memberspace's alone, each a DID, reading and writing said.
        (
            meta_archive(
                &forum,
                personal.clone(),
                &[(&alice_key, alice_member.clone())],
                &[],
            ),
            "InvalidArchive",
        ),
        (
            meta_archive(
                &forum,
                memberspace(true),
                &[("com.example.other/x", alice_member.clone())],
                &[],
            ),
            "InvalidArchive",
        ),
        (
            meta_archive(
                &forum,
                memberspace(true),
                &[("com.atproto.memberspace.member/bob", alice_member)],
                &[],
            ),
            "InvalidArchive",
        ),
        (
            meta_archive(
                &forum,
                memberspace(true),
                &[(&alice_key, json!({"read": true}))],
                &[],
            ),
            "InvalidArchive",
        ),
    ] {
        assert_eq!(refused("meta", archive).await, is(why), "{why}");
    }
    assert!(w.h.host.store.space(&forum).unwrap().is_none());

    let notes = [("com.example.note/1", note("one"))];
    let outside = [("note", note("one"))];
    for (archive, why) in [
        (b"not an archive".to_vec(), "InvalidArchive"),
        (
            space_repo(&alice.did, &forum, &notes, None),
            "NotSpaceOwner",
        ),
        // Its own space's repo waits for the space.
        (space_repo(&w.did, &forum, &notes, None), "NotFound"),
        (
            space_repo(&w.did, "general", &notes, None),
            "InvalidArchive",
        ),
        (
            space_repo(&w.did, &others, &notes, Some([0; 32])),
            "InvalidArchive",
        ),
        (
            space_repo(&w.did, &others, &outside, None),
            "InvalidArchive",
        ),
    ] {
        assert_eq!(refused("repo", archive).await, is(why), "{why}");
    }
}

#[test]
fn reads_a_members_did_from_its_record_key() {
    assert_eq!(
        record_key_to_did("did:web:localhost:2583"),
        "did:web:localhost%3A2583"
    );
    for same in [
        "did:plc:alice",
        "did:web:example.com",
        "did:web::2583",
        "did:web:localhost:",
        "did:web:localhost:25a",
    ] {
        assert_eq!(record_key_to_did(same), same);
    }
}

#[test]
fn makes_an_address_label_of_a_handles_first_label() {
    assert_eq!(label_of("Harbor_Cycling.bsky.social"), "harbor-cycling");
    assert_eq!(label_of("-x-.example"), "x");
    assert_eq!(label_of("a__b"), "a-b");
    assert_eq!(label_of(&format!("{}-b", "a".repeat(62))), "a".repeat(62));
    assert_eq!(label_of(""), "");
}
