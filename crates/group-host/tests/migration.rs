//! A group moved from one group host to another: prepared at the source,
//! taken in at the destination, handed over in the directory, and switched
//! on; and what the destination will not take in.

mod common;

use atproto::mst::Tree;
use atproto::plc::{Op, Signed, tombstone};
use atproto::service_auth::{self, Claims};
use atproto::space::{Commit as SpaceCommit, SPACE_META, SPACE_META_VERSION, star, write_star};
use atproto::{Cid, Commit, Curve, Keypair, car, drisl};
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get as route;
use common::groups::*;
use common::oauth::{App, give_avatar, make_space};
use common::spaces::*;
use common::{Running, serve, start_reachable};
use group_host::group::{members_space, meta_space};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

const PREPARE: &str = "org.atgrp.internal.migratePrepare";
const RECEIVE: &str = "org.atgrp.internal.migrateReceive";
const COMPLETE: &str = "org.atgrp.internal.migrateComplete";
const ACTIVATE: &str = "org.atgrp.internal.migrateActivate";

/// The host a group leaves, with Riders, Alice's, Bob in it, a picture, a
/// rule, a post, and a calendar Carol writes in and Bob may write in by a
/// grant; and the host it goes to, on the same directory.
async fn two_hosts() -> (Running, Running, String, String, [Authority; 3]) {
    two_hosts_with(&[]).await
}

/// As `two_hosts`, the host it goes to with `vars` besides.
async fn two_hosts_with(
    vars: &[(&str, &str)],
) -> (Running, Running, String, String, [Authority; 3]) {
    let a = start_reachable(&[]).await;
    let plc = plc_url(&a);
    let mut b_vars = vec![
        ("PLC_URL", plc.as_str()),
        ("HANDLE_DOMAIN", ".groups.b.test"),
    ];
    b_vars.extend_from_slice(vars);
    let b = start_reachable(&b_vars).await;
    let [alice, bob, carol] = people(&a).await;
    let did = start_group(
        &a,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    give_avatar(&a, &alice, &did, "Riders", b"face").await;
    let console = App::new("console");
    let members = members_space(&did);
    let token = console.token_for(&a, &alice, &did, &members).await;
    let rule = json!({"repo": did, "space": members, "collection": format!("{NS}.rule"), "record": {"$type": format!("{NS}.rule"), "title": "Be kind", "text": "To each other."}});
    let said = console
        .call(&a, "com.atproto.space.createRecord", &token, Some(&rule))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let readers = json!({"$type": "org.atgrp.policy", "grants": [{"role": "member", "read": true, "write": true}]});
    let calendar = make_space(&a, &alice, &did, "com.example.calendar", "rides", readers).await;
    let post = group_host::repo::Write::Create {
        collection: "app.bsky.feed.post".into(),
        rkey: "ride".into(),
        value: json!({"$type": "app.bsky.feed.post", "text": "Ride on Saturday", "createdAt": "2026-11-03T00:00:00Z"}),
    };
    a.host.write(&did, &[post]).unwrap();
    a.host
        .record_writer(&calendar, &carol.did, "3lzrevzzzzzzz", &[7u8; 32])
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
    let about = b
        .host
        .group_record(&did, &meta_space(&did), &format!("{NS}.about"), "self")
        .unwrap()
        .unwrap();
    assert_eq!(about["displayName"], "Riders");
    let rules = b
        .host
        .group_records(&did, &members_space(&did), &format!("{NS}.rule"))
        .unwrap();
    assert_eq!(rules[0].1["title"], "Be kind");
    assert!(
        b.host
            .store
            .record(&did, "app.bsky.feed.post", "ride")
            .unwrap()
            .is_some()
    );
    let face = Cid::parse(about["avatar"]["ref"]["$link"].as_str().unwrap()).unwrap();
    assert_eq!(b.host.read_blob(&did, &face).unwrap().unwrap().1, b"face");
    let writers = b.host.store.writers(&calendar, None, 100).unwrap();
    assert!(
        writers
            .iter()
            .any(|w| w.did == carol.did && w.repo_rev == "3lzrevzzzzzzz" && w.data == [7u8; 32])
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
    let lxm = format!("{INTERNAL}.addRecoveryKey");
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
    // Nor is there anything more to pull.
    let token = prepared["exportToken"].as_str().unwrap();
    let (status, _) = a
        .get(
            "org.atgrp.internal.migrateExport",
            &format!("group={did}&token={token}&part=blobs"),
        )
        .await;
    assert_eq!(status, 401);

    let (status, on) = call(&b, &alice, ACTIVATE, json!({"did": did})).await;
    assert_eq!(status, 200, "{on}");
    assert_eq!(on["spaces"], spaces.len());
    assert!(b.host.deactivation(&did).unwrap().is_none());
    // The group's own repo in each of its spaces is told once it is on.
    for space in spaces.iter().filter_map(Json::as_str) {
        let writers = b.host.store.writers(space, None, 100).unwrap();
        assert!(writers.iter().any(|w| w.did == did), "{space}: {writers:?}");
    }
    let lxm = format!("{INTERNAL}.getGroup");
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
            "org.atgrp.internal.migrateExport",
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

/// A resolver answering for the handles in its map.
async fn names(
    State(names): State<Arc<Mutex<HashMap<String, String>>>>,
    Query(q): Query<HashMap<String, String>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match names.lock().unwrap().get(&q["handle"]) {
        Some(did) => axum::Json(json!({ "did": did })).into_response(),
        None => axum::http::StatusCode::BAD_REQUEST.into_response(),
    }
}

/// Two hosts, the second asking a resolver of the test's own who a
/// handle names; and Riders' snapshot saying it went by its own domain.
async fn by_its_own_domain() -> (
    [Running; 2],
    Json,
    String,
    Arc<Mutex<HashMap<String, String>>>,
    Authority,
) {
    let known = Arc::new(Mutex::new(HashMap::new()));
    let router = Router::new()
        .route("/xrpc/com.atproto.identity.resolveHandle", route(names))
        .with_state(known.clone());
    let resolver = serve(router).await;
    let (a, b, did, _, [alice, ..]) = two_hosts_with(&[("HANDLE_RESOLVERS", &resolver)]).await;
    let (_, prepared) = call(
        &a,
        &alice,
        PREPARE,
        json!({"group": did, "destDid": b.host.config.service_did}),
    )
    .await;
    let mut snapshot = prepared["snapshot"].clone();
    snapshot["handle"] = json!("riders.example.com");
    snapshot["address"] = json!(RIDERS);
    let receive = json!({"did": did, "serviceJwt": prepared["serviceJwt"], "snapshot": snapshot, "exportToken": prepared["exportToken"]});
    ([a, b], receive, did, known, alice)
}

#[tokio::test]
async fn takes_a_domain_of_its_own_along_only_while_it_names_the_group() {
    let ([_a, b], receive, did, _, alice) = by_its_own_domain().await;
    // The snapshot says so; nothing else does.
    let (status, said) = call(&b, &alice, RECEIVE, receive).await;
    assert_eq!(status, 200, "{said}");
    let account = b.host.store.account(&did).unwrap().unwrap();
    assert_eq!(account.handle, "riders.groups.b.test");
}

#[tokio::test]
async fn takes_in_no_group_whose_own_domain_another_has_here() {
    let ([_a, b], receive, did, known, alice) = by_its_own_domain().await;
    known
        .lock()
        .unwrap()
        .insert("riders.example.com".into(), did.clone());
    // A group here has that domain now.
    let here = start_group(
        &b,
        &alice,
        json!({"handle": "others.groups.b.test", "displayName": "Others"}),
    )
    .await;
    b.host
        .store
        .set_handle(&here, "riders.example.com", &[])
        .unwrap();
    let said = call(&b, &alice, RECEIVE, receive.clone()).await;
    assert_eq!(error(&said), (400, "HandleNotAvailable"), "{}", said.1);
    assert!(b.host.store.account(&did).unwrap().is_none());
    // Free again, it comes along, with its address beside it.
    b.host
        .store
        .set_handle(&here, "others.groups.b.test", &[])
        .unwrap();
    let (status, said) = call(&b, &alice, RECEIVE, receive).await;
    assert_eq!(status, 200, "{said}");
    let account = b.host.store.account(&did).unwrap().unwrap();
    assert_eq!(account.handle, "riders.example.com");
    assert_eq!(
        b.host.store.address(&did).unwrap().as_deref(),
        Some("riders.groups.b.test")
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
    // Keys the directory does not take are the caller's to change, not a
    // fault of the host's.
    let said = call(
        &a,
        &alice,
        COMPLETE,
        json!({"group": did, "recommendedCreds": creds}),
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidOperation"), "{}", said.1);
    assert!(
        said.1["message"]
            .as_str()
            .unwrap()
            .contains("the PLC directory refused it (400 Bad Request)"),
        "{}",
        said.1
    );
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

/// Its repo, or, once `said` has `repo-gone`, a 404 of the PDS's.
async fn fake_repo(State(f): State<Arc<Fake>>) -> (axum::http::StatusCode, Vec<u8>) {
    let said = f.said.lock().unwrap();
    if said.contains_key("repo-gone") {
        let body = br#"{"error":"RepoNotFound","message":"no such repo"}"#;
        return (axum::http::StatusCode::NOT_FOUND, body.to_vec());
    }
    let repo = said.get("repo").cloned().unwrap_or_default();
    (axum::http::StatusCode::OK, repo)
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
        .route("/xrpc/org.atgrp.internal.migrateExport", route(fake_export))
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
    // Its space's meta archive and repo, as its commit says, unless a test
    // says otherwise.
    {
        let mut said = fake.said.lock().unwrap();
        said.entry("blobs".into())
            .or_insert_with(|| br#"{"blobs": []}"#.to_vec());
        // Under a policy granting nobody anything, unless a test gives one.
        let policy = said.get("policy").map_or_else(
            || json!({"$type": "org.atgrp.policy", "grants": []}),
            |p| serde_json::from_slice(p).unwrap(),
        );
        said.entry("meta".into())
            .or_insert_with(|| space_meta_of(&space, &policy));
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

/// The meta archive of `space`, a space under `policy`, with no writers.
fn space_meta_of(space: &str, policy: &Json) -> Vec<u8> {
    let meta = BTreeMap::from([
        ("$type".to_string(), drisl::Value::String(SPACE_META.into())),
        ("ver".to_string(), drisl::Value::Integer(SPACE_META_VERSION)),
        ("space".to_string(), drisl::Value::String(space.into())),
    ]);
    let policy = drisl::encode(&drisl::Value::from_json(policy).unwrap());
    write_star(&meta, [(b"policy".as_slice(), policy.as_slice())])
}

/// A repo of one record, as `did`, signed by `key`.
fn repo_of(did: &str, key: &Keypair) -> Vec<u8> {
    let record =
        drisl::encode(&drisl::Value::from_json(&json!({"$type": "a.b.c", "x": 1})).unwrap());
    let cid = Cid::of_cbor(&record);
    let tree = Tree::build(&BTreeMap::from([("a.b.c/r".to_string(), cid)]));
    let commit = Commit::sign(did, tree.root(), "3l5mz5mkbn22b", key);
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
    // A repo its host says it has not: said so, not read as one.
    let fake = Arc::new(Fake::default());
    fake.said.lock().unwrap().insert("repo-gone".into(), vec![]);
    let said = from_fake(&b, &alice, fake, &key).await;
    assert_eq!(error(&said), (502, "SourceUnreadable"), "{}", said.1);
    let message = said.1["message"].as_str().unwrap();
    assert!(
        message.contains("getRepo answered 404 Not Found"),
        "{message}"
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
    // A space under a policy of this host's that is not as its lexicon
    // has it.
    let fake = Arc::new(Fake::default());
    let policy = json!({"$type": "org.atgrp.policy", "grants": "everyone"});
    fake.said
        .lock()
        .unwrap()
        .insert("policy".into(), policy.to_string().into_bytes());
    let said = from_fake(&b, &alice, fake, &key).await;
    assert_eq!(error(&said), (400, "InvalidArchive"), "{}", said.1);
}

#[tokio::test]
async fn takes_in_only_what_it_may_and_no_more_than_it_keeps() {
    // A host that starts groups for someone else takes in none of Alice's.
    let (_, b, _, _, [alice, ..]) =
        two_hosts_with(&[("GROUP_FOUNDERS", "did:plc:someoneelse")]).await;
    let key = Keypair::generate(Curve::K256);
    let said = from_fake(&b, &alice, Arc::new(Fake::default()), &key).await;
    assert_eq!(error(&said), (403, "NotAllowed"), "{}", said.1);
    // A file larger than the host keeps stops the move, unread.
    let (_, b, _, _, [alice, ..]) = two_hosts_with(&[("BLOB_UPLOAD_LIMIT", "4")]).await;
    let fake = Arc::new(Fake::default());
    let big = b"more than four".to_vec();
    fake.said.lock().unwrap().insert(
        "blobs".into(),
        json!({"blobs": [{"cid": Cid::of_raw(&big).to_string(), "mimeType": "image/png"}]})
            .to_string()
            .into_bytes(),
    );
    fake.said.lock().unwrap().insert("blob".into(), big);
    let said = from_fake(&b, &alice, fake, &key).await;
    assert_eq!(error(&said), (502, "SourceUnreadable"), "{}", said.1);
    assert!(
        said.1["message"]
            .as_str()
            .unwrap()
            .contains("more than 4 bytes"),
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
