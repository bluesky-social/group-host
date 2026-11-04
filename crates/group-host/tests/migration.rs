//! A group moved from one group host to another: prepared at the source,
//! taken in at the destination, handed over in the directory, and switched
//! on; and what the destination will not take in.

mod common;

use atproto::mst::Tree;
use atproto::plc::{Op, Signed, tombstone};
use atproto::service_auth::{self, Claims};
use atproto::space::{Commit as SpaceCommit, star};
use atproto::{Cid, Commit, Curve, Keypair, car, drisl};
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get as route;
use common::groups::*;
use common::spaces::*;
use common::{Running, serve, start_reachable};
use group_host::group::meta_space;
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

const PREPARE: &str = "fyi.opensocial.migratePrepare";
const RECEIVE: &str = "fyi.opensocial.migrateReceive";
const COMPLETE: &str = "fyi.opensocial.migrateComplete";
const ACTIVATE: &str = "fyi.opensocial.migrateActivate";

/// The host a group leaves, with Riders, Alice's, Bob in it, a picture, a
/// rule, a post, and a calendar Carol writes in and Bob may write in by a
/// grant; and the host it goes to, on the same directory.
async fn two_hosts() -> (Running, Running, String, String, [Authority; 3]) {
    let a = start_reachable(&[]).await;
    let plc = plc_url(&a);
    let b = start_reachable(&[
        ("PLC_URL", plc.as_str()),
        ("HANDLE_DOMAIN", ".groups.b.test"),
    ])
    .await;
    let [alice, bob, carol] = people(&a).await;
    let did = start_group(
        &a,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let lxm = format!("{NS}.uploadImage");
    let (k, v) = auth(&a, &alice, &lxm);
    let res = a
        .http
        .post(format!("{}/xrpc/{lxm}?group={did}", a.url))
        .header(k, v)
        .header("content-type", "image/png")
        .body(b"face".to_vec())
        .send()
        .await
        .unwrap();
    let image: Json = res.json().await.unwrap();
    let calls = [
        (
            "updateProfile",
            json!({"group": did, "displayName": "Riders", "avatar": image["blob"]}),
        ),
        ("putRule", json!({"group": did, "title": "Be kind"})),
        (
            "createSpace",
            json!({"group": did, "spaceType": "com.example.calendar", "skey": "rides", "readableBy": [{"role": "member"}]}),
        ),
    ];
    let mut calendar = String::new();
    for (method, body) in calls {
        let lxm = format!("{NS}.{method}");
        let (status, said) = post(&a, &lxm, &body, &[auth(&a, &alice, &lxm)]).await;
        assert_eq!(status, 200, "{method}: {said}");
        if method == "createSpace" {
            calendar = said["uri"].as_str().unwrap().to_string();
        }
    }
    let post = group_host::repo::Write::Create {
        collection: "app.bsky.feed.post".into(),
        rkey: "ride".into(),
        value: json!({"$type": "app.bsky.feed.post", "text": "Ride on Saturday", "createdAt": "2026-11-03T00:00:00Z"}),
    };
    a.host.write(&did, &[post]).unwrap();
    a.host
        .record_writer(&calendar, &did, &carol.did, "3lzrev", &[7u8; 32])
        .unwrap();
    a.host.grant(&calendar, &bob.did, true, true).unwrap();
    (a, b, did, calendar, [alice, bob, carol])
}

async fn call(h: &Running, who: &Authority, lxm: &str, body: Json) -> (u16, Json) {
    post(h, lxm, &body, &[auth(h, who, lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

#[tokio::test]
async fn moves_a_group_whole_to_another_host() {
    let (a, b, did, calendar, [alice, bob, carol]) = two_hosts().await;
    let (status, prepared) = call(
        &a,
        &alice,
        PREPARE,
        json!({"group": did, "destDid": b.host.config.service_did}),
    )
    .await;
    assert_eq!(status, 200, "{prepared}");
    assert_eq!(prepared["snapshot"]["handle"], RIDERS);
    let spaces = prepared["snapshot"]["spaces"].as_array().unwrap();
    assert!(spaces.contains(&json!(calendar)) && spaces.contains(&json!(meta_space(&did))));

    let receive = json!({"did": did, "serviceJwt": prepared["serviceJwt"], "snapshot": prepared["snapshot"], "exportToken": prepared["exportToken"]});
    let (status, received) = call(&b, &alice, RECEIVE, receive.clone()).await;
    assert_eq!(status, 200, "{received}");
    assert_eq!(received["spaces"], spaces.len());
    assert_eq!(received["blobs"], 1);
    let creds = &received["recommendedCreds"];
    assert_eq!(creds["services"]["atproto_pds"]["endpoint"], b.url.as_str());
    assert_eq!(creds["alsoKnownAs"], json!(["at://riders.groups.b.test"]));

    // Everything there, switched off until the directory names it.
    let account = b.host.store.account(&did).unwrap().unwrap();
    assert_eq!(account.handle, "riders.groups.b.test");
    assert!(b.host.deactivation(&did).unwrap().is_some());
    let profile = b
        .host
        .group_record(&did, &meta_space(&did), &format!("{NS}.profile"), "self")
        .unwrap()
        .unwrap();
    assert_eq!(profile["displayName"], "Riders");
    let rules = b
        .host
        .group_records(&did, &meta_space(&did), &format!("{NS}.rule"))
        .unwrap();
    assert_eq!(rules[0].1["title"], "Be kind");
    assert!(
        b.host
            .store
            .record(&did, "app.bsky.feed.post", "ride")
            .unwrap()
            .is_some()
    );
    let face = Cid::parse(profile["avatar"]["ref"]["$link"].as_str().unwrap()).unwrap();
    assert_eq!(b.host.read_blob(&did, &face).unwrap().unwrap().1, b"face");
    let writers = b.host.store.writers(&calendar, None, 100).unwrap();
    assert!(
        writers
            .iter()
            .any(|w| w.did == carol.did && w.repo_rev == "3lzrev" && w.data == [7u8; 32])
    );
    assert_eq!(
        b.host.store.granted(&calendar, &bob.did).unwrap(),
        Some((true, true))
    );
    assert!(
        b.host
            .roles(&did, &alice.did)
            .unwrap()
            .contains(&"admin".to_string())
    );
    // Taken in once.
    assert_eq!(
        error(&call(&b, &alice, RECEIVE, receive).await),
        (400, "AlreadyHere")
    );
    // Not switched on before the directory names it.
    assert_eq!(
        error(&call(&b, &alice, ACTIVATE, json!({"did": did})).await),
        (400, "NotYet")
    );

    // The source hands it over, and keeps its stewards ahead.
    let steward = Keypair::generate(Curve::K256).public().did_key();
    let lxm = format!("{NS}.addRecoveryKey");
    assert_eq!(
        post(
            &a,
            &lxm,
            &json!({"group": did, "key": steward}),
            &[auth(&a, &alice, &lxm)]
        )
        .await
        .0,
        200
    );
    let (status, done) = call(
        &a,
        &alice,
        COMPLETE,
        json!({"group": did, "recommendedCreds": creds}),
    )
    .await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["movedTo"], b.url.as_str());
    let Signed::Update { op, .. } = a.host.plc.last_op(&did).await.unwrap() else {
        panic!()
    };
    assert_eq!(
        op.rotation_keys,
        [steward, b.host.secrets.rotation.public().did_key()]
    );
    assert_eq!(op.services["atproto_pds"].endpoint, b.url);
    assert!(a.host.deactivation(&did).unwrap().is_some());
    // Handed over, it is the source's no more to hand.
    let said = call(
        &a,
        &alice,
        COMPLETE,
        json!({"group": did, "recommendedCreds": creds}),
    )
    .await;
    assert_eq!(error(&said), (400, "NotController"));
    let said = call(
        &a,
        &alice,
        PREPARE,
        json!({"group": did, "destDid": b.host.config.service_did}),
    )
    .await;
    assert_eq!(error(&said), (400, "NotController"));

    let (status, on) = call(&b, &alice, ACTIVATE, json!({"did": did})).await;
    assert_eq!(status, 200, "{on}");
    assert_eq!(on["spaces"], spaces.len());
    assert!(b.host.deactivation(&did).unwrap().is_none());
    let lxm = format!("{NS}.getGroup");
    let (_, view) = get(&b, &lxm, &format!("group={did}"), &[auth(&b, &alice, &lxm)]).await;
    assert_eq!(view["group"]["displayName"], "Riders");
    assert_eq!(view["group"]["handle"], "riders.groups.b.test");
}

#[tokio::test]
async fn prepares_only_for_one_who_configures_and_exports_only_with_the_token() {
    let (a, b, did, calendar, [alice, bob, _]) = two_hosts().await;
    let dest = b.host.config.service_did.clone();
    assert_eq!(
        error(&call(&a, &bob, PREPARE, json!({"group": did, "destDid": dest})).await),
        (403, "Forbidden")
    );
    assert_eq!(
        error(
            &call(
                &a,
                &alice,
                PREPARE,
                json!({"group": did, "destDid": "nowhere"})
            )
            .await
        ),
        (400, "InvalidRequest")
    );
    assert_eq!(
        error(&call(&a, &alice, PREPARE, json!({"destDid": dest})).await),
        (400, "InvalidRequest")
    );
    let (_, prepared) = call(&a, &alice, PREPARE, json!({"group": did, "destDid": dest})).await;
    let token = prepared["exportToken"].as_str().unwrap();
    let export = |q: String| {
        a.get(
            "fyi.opensocial.migrateExport",
            Box::leak(q.into_boxed_str()),
        )
    };
    assert_eq!(
        export(format!("group={did}&token=wrong&part=blobs"))
            .await
            .0,
        401
    );
    for q in [
        format!("group={did}&token={token}&part=other&space={calendar}"),
        format!(
            "group={did}&token={token}&part=repo&space=at://{}/space/x.y.z/k",
            bob.did
        ),
        format!("group={did}&token={token}&part=blob&cid=nonsense"),
    ] {
        assert_eq!(export(q.clone()).await.0, 400, "{q}");
    }
    let unknown = Cid::of_raw(b"nothing");
    let (status, said) = export(format!("group={did}&token={token}&part=blob&cid={unknown}")).await;
    assert_eq!(
        (status, said["error"].as_str()),
        (400, Some("BlobNotFound"))
    );
}

#[tokio::test]
async fn takes_in_only_for_one_who_configures_by_the_groups_own_records() {
    let (a, b, did, _, [alice, bob, carol]) = two_hosts().await;
    let dest = b.host.config.service_did.clone();
    // Preparing again replaces the token: the one for elsewhere first.
    let (_, elsewhere) = call(
        &a,
        &alice,
        PREPARE,
        json!({"group": did, "destDid": "did:web:elsewhere.test"}),
    )
    .await;
    let (_, prepared) = call(&a, &alice, PREPARE, json!({"group": did, "destDid": dest})).await;
    let receive = json!({"did": did, "serviceJwt": prepared["serviceJwt"], "snapshot": prepared["snapshot"], "exportToken": prepared["exportToken"]});
    assert_eq!(
        error(&call(&b, &bob, RECEIVE, receive.clone()).await),
        (403, "Forbidden")
    );
    // Nothing half taken in stays.
    assert!(b.host.store.account(&did).unwrap().is_none());

    // Not without the group's own token for here, or the source's.
    let other = Keypair::generate(Curve::K256);
    let forged = service_auth::sign(
        &other,
        &Claims {
            iss: did.clone(),
            aud: dest.clone(),
            exp: now() + 60,
            iat: Some(now()),
            lxm: Some(RECEIVE.into()),
            jti: None,
        },
    );
    let with = |k: &str, v: Json| {
        let mut body = receive.clone();
        body[k] = v;
        body
    };
    for (body, refused) in [
        (with("serviceJwt", json!(forged)), (400, "BadServiceJwt")),
        (
            with("serviceJwt", elsewhere["serviceJwt"].clone()),
            (400, "BadServiceJwt"),
        ),
        (
            with("exportToken", json!("wrong")),
            (502, "SourceUnreadable"),
        ),
        (
            with("snapshot", json!({"spaces": []})),
            (400, "InvalidRequest"),
        ),
        (
            with(
                "snapshot",
                json!({"handle": RIDERS, "spaces": ["not a space"]}),
            ),
            (400, "InvalidRequest"),
        ),
        (
            with(
                "snapshot",
                json!({"handle": RIDERS, "spaces": [format!("at://{}/space/x.y.z/k", carol.did)]}),
            ),
            (400, "InvalidRequest"),
        ),
        (
            json!({"serviceJwt": "x", "exportToken": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"did": did, "exportToken": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"did": did, "serviceJwt": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            with("did", json!("did:plc:nobodynobodynobodyxx")),
            (502, "SourceUnreadable"),
        ),
    ] {
        assert_eq!(
            error(&call(&b, &alice, RECEIVE, body.clone()).await),
            refused,
            "{body}"
        );
        assert!(b.host.store.account(&did).unwrap().is_none());
    }
    // A handle taken here meanwhile.
    b.host.create_account("riders.groups.b.test").await.unwrap();
    assert_eq!(
        error(&call(&b, &alice, RECEIVE, receive).await),
        (400, "HandleNotAvailable")
    );
}

#[tokio::test]
async fn hands_over_only_what_names_the_destination() {
    let (a, _, did, _, [alice, ..]) = two_hosts().await;
    let creds = json!({"rotationKeys": ["did:key:x"], "verificationMethods": {"atproto": "did:key:y"}, "services": {"atproto_pds": {"endpoint": "https://b.test"}}});
    let with_out = |path: &[&str]| {
        let mut c = creds.clone();
        let mut at = &mut c;
        for p in &path[..path.len() - 1] {
            at = &mut at[*p];
        }
        at.as_object_mut().unwrap().remove(*path.last().unwrap());
        c
    };
    for missing in [
        &["services"][..],
        &["verificationMethods", "atproto"],
        &["rotationKeys"],
    ] {
        let said = call(
            &a,
            &alice,
            COMPLETE,
            json!({"group": did, "recommendedCreds": with_out(missing)}),
        )
        .await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{missing:?}");
    }
    // An ended DID is nobody's to hand over.
    let last = a.host.plc.last_op(&did).await.unwrap();
    a.host
        .plc
        .publish(&did, &tombstone(last.cid(), &a.host.secrets.rotation))
        .await
        .unwrap();
    let said = call(
        &a,
        &alice,
        COMPLETE,
        json!({"group": did, "recommendedCreds": creds}),
    )
    .await;
    assert_eq!(error(&said), (400, "NotController"));
}

/// A host a group says it is on, answering as `said` makes it.
#[derive(Default)]
struct Fake {
    said: Mutex<HashMap<String, Vec<u8>>>,
}

async fn fake_repo(State(f): State<Arc<Fake>>) -> Vec<u8> {
    f.said
        .lock()
        .unwrap()
        .get("repo")
        .cloned()
        .unwrap_or_default()
}

async fn fake_export(
    State(f): State<Arc<Fake>>,
    Query(q): Query<HashMap<String, String>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let part = q["part"].clone();
    // The export's repo part is the space's STAR; getRepo serves the CAR.
    let key = if part == "repo" {
        "repo-space".to_string()
    } else {
        part.clone()
    };
    let body = f
        .said
        .lock()
        .unwrap()
        .get(&key)
        .cloned()
        .unwrap_or_default();
    match part.as_str() {
        "blob" | "repo" => body.into_response(),
        _ => ([("content-type", "application/json")], body).into_response(),
    }
}

/// A group on a host of the test's own making, signed by `key`, which
/// asks the destination to take it in: what the destination says.
async fn from_fake(b: &Running, alice: &Authority, fake: Arc<Fake>, key: &Keypair) -> (u16, Json) {
    let router = Router::new()
        .route("/xrpc/com.atproto.sync.getRepo", route(fake_repo))
        .route("/xrpc/fyi.opensocial.migrateExport", route(fake_export))
        .with_state(fake.clone());
    let url = serve(router).await;
    let rotation = Keypair::generate(Curve::K256);
    let genesis = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        "faked.groups.test",
        &url,
    )
    .sign(None, &rotation);
    let did = genesis.did();
    b.http
        .post(format!("{}/{did}", plc_url(b)))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    let jwt = service_auth::sign(
        key,
        &Claims {
            iss: did.clone(),
            aud: b.host.config.service_did.clone(),
            exp: now() + 60,
            iat: Some(now()),
            lxm: Some(RECEIVE.into()),
            jti: None,
        },
    );
    let space = format!("at://{did}/space/com.example.calendar/rides");
    // Its space's repo, as its commit says, unless a test says otherwise.
    {
        let mut said = fake.said.lock().unwrap();
        said.entry("blobs".into())
            .or_insert_with(|| br#"{"blobs": []}"#.to_vec());
        said.entry("meta".into())
            .or_insert_with(|| br#"{"writers": [], "grants": []}"#.to_vec());
        if !said.contains_key("repo-space") {
            let record = drisl::encode(
                &drisl::Value::from_json(&json!({"$type": "c.d.e", "x": 1})).unwrap(),
            );
            let mut hash = atproto::space::LtHash::default();
            hash.add(&atproto::space::element(
                "c.d.e",
                "r",
                &Cid::of_cbor(&record),
            ));
            let commit = SpaceCommit {
                repo: did.clone(),
                space: space.clone(),
                rev: "3lz".into(),
                data: hash.digest(),
            };
            said.insert(
                "repo-space".into(),
                star(&commit, &BTreeMap::from([("c.d.e/r".to_string(), record)])),
            );
        }
        let repo = said
            .get("repo")
            .cloned()
            .unwrap_or_else(|| repo_of(&did, key));
        said.insert("repo".into(), repo);
    }
    let body = json!({"did": did, "serviceJwt": jwt, "exportToken": "t", "snapshot": {"handle": "faked.groups.test", "spaces": [space]}});
    call(b, alice, RECEIVE, body).await
}

/// A repo of one record, as `did`, signed by `key`.
fn repo_of(did: &str, key: &Keypair) -> Vec<u8> {
    let record =
        drisl::encode(&drisl::Value::from_json(&json!({"$type": "a.b.c", "x": 1})).unwrap());
    let cid = Cid::of_cbor(&record);
    let tree = Tree::build(&BTreeMap::from([("a.b.c/r".to_string(), cid)]));
    let commit = Commit::sign(did, tree.root(), "3lz", key);
    let mut blocks: Vec<(Cid, Vec<u8>)> = tree.blocks().into_iter().collect();
    blocks.push((cid, record));
    blocks.push((commit.cid(), commit.to_bytes()));
    car::write(
        &[commit.cid()],
        blocks.iter().map(|(c, b)| (c, b.as_slice())),
    )
}

#[tokio::test]
async fn takes_in_nothing_it_cannot_check() {
    let (_, b, _, _, [alice, ..]) = two_hosts().await;
    let key = Keypair::generate(Curve::K256);
    // A repo another key signed.
    let fake = Arc::new(Fake::default());
    fake.said.lock().unwrap().insert(
        "repo".into(),
        repo_of("did:plc:x", &Keypair::generate(Curve::K256)),
    );
    let said = from_fake(&b, &alice, fake, &key).await;
    assert_eq!(error(&said), (502, "SourceUnreadable"), "{}", said.1);
    assert!(
        said.1["message"]
            .as_str()
            .unwrap()
            .contains("signed and whole")
    );
    // A file that is not what it says.
    let fake = Arc::new(Fake::default());
    let other = Cid::of_raw(b"other");
    fake.said.lock().unwrap().insert(
        "blobs".into(),
        json!({"blobs": [{"cid": other.to_string(), "mimeType": "image/png"}]})
            .to_string()
            .into_bytes(),
    );
    fake.said
        .lock()
        .unwrap()
        .insert("blob".into(), b"not that".to_vec());
    let said = from_fake(&b, &alice, fake, &key).await;
    assert!(
        said.1["message"]
            .as_str()
            .unwrap()
            .contains("is not what it says"),
        "{}",
        said.1
    );
    // A space's repo that is not what its commit says.
    let fake = Arc::new(Fake::default());
    fake.said
        .lock()
        .unwrap()
        .insert("repo-space".into(), b"not a star".to_vec());
    let said = from_fake(&b, &alice, fake, &key).await;
    assert!(
        said.1["message"]
            .as_str()
            .unwrap()
            .contains("not what its commit says"),
        "{}",
        said.1
    );
}

#[test]
fn takes_a_handle_here() {
    use group_host::migration::handle_here;
    let domain = ".groups.b.test";
    // Its address at the host it leaves: the same label here.
    assert_eq!(
        handle_here("riders.groups.test", Some("riders.groups.test"), domain),
        ("riders.groups.b.test".into(), "riders.groups.b.test".into())
    );
    assert_eq!(
        handle_here("riders.groups.test", None, domain),
        ("riders.groups.b.test".into(), "riders.groups.b.test".into())
    );
    // A domain of its own: kept.
    assert_eq!(
        handle_here("riders.example.com", Some("riders.groups.test"), domain),
        ("riders.example.com".into(), "riders.groups.b.test".into())
    );
}

#[tokio::test]
async fn takes_in_no_group_whose_did_names_no_host() {
    let (_, b, _, _, [alice, ..]) = two_hosts().await;
    let key = Keypair::generate(Curve::K256);
    let mut op = Op::atproto(
        &key.public().did_key(),
        &[key.public().did_key()],
        "x.groups.test",
        "https://x.test",
    );
    op.services.clear();
    let genesis = op.sign(None, &key);
    let did = genesis.did();
    b.http
        .post(format!("{}/{did}", plc_url(&b)))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    let body = json!({"did": did, "serviceJwt": "x", "exportToken": "x", "snapshot": {"handle": "x.groups.test", "spaces": []}});
    let said = call(&b, &alice, RECEIVE, body).await;
    assert_eq!(error(&said), (502, "SourceUnreadable"));
    assert!(
        said.1["message"]
            .as_str()
            .unwrap()
            .contains("names no key, or no host")
    );
}
