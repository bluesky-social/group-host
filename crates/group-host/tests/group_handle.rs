//! A group's handle on a domain of its own, and its own address back.

mod common;

use atproto::plc::Signed;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get as route_get;
use common::groups::*;
use common::spaces::*;
use common::{Running, serve, start, turn};
use serde_json::{Value as Json, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A service resolving handles from what it is told, as one reading DNS.
#[derive(Default)]
struct Names(Mutex<HashMap<String, String>>);

async fn resolve(
    State(names): State<Arc<Names>>,
    Query(q): Query<HashMap<String, String>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let named = names.0.lock().unwrap().get(&q["handle"]).cloned();
    match named {
        Some(did) => axum::Json(json!({"did": did})).into_response(),
        None => axum::http::StatusCode::BAD_REQUEST.into_response(),
    }
}

/// A host resolving handles by `/.well-known` (which finds nothing under
/// `.test`) and then the service `names` is, and a group Alice started.
async fn here() -> (Running, Arc<Names>, String, [Authority; 3]) {
    let names = Arc::new(Names::default());
    let router = Router::new()
        .route(
            "/xrpc/com.atproto.identity.resolveHandle",
            route_get(resolve),
        )
        .with_state(names.clone());
    let service = serve(router).await;
    let resolvers = format!("well-known,{service}");
    let h = start(&[("HANDLE_RESOLVERS", resolvers.as_str())], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    (h, names, did, [alice, bob, carol])
}

async fn check(h: &Running, who: &Authority, did: &str, handle: &str) -> (u16, Json) {
    let lxm = format!("{INTERNAL}.checkGroupHandle");
    get(
        h,
        &lxm,
        &format!("group={did}&handle={handle}"),
        &[auth(h, who, &lxm)],
    )
    .await
}

async fn update(h: &Running, who: &Authority, did: &str, handle: &str) -> (u16, Json) {
    let lxm = format!("{INTERNAL}.updateGroupHandle");
    post(
        h,
        &lxm,
        &json!({"group": did, "handle": handle}),
        &[auth(h, who, &lxm)],
    )
    .await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

#[tokio::test]
async fn resolves_anyones_handle_for_someone_signed_in() {
    let (h, names, riders, [alice, ..]) = here().await;
    names
        .0
        .lock()
        .unwrap()
        .insert("alice.example".into(), alice.did.clone());
    let lxm = "com.atproto.identity.resolveHandle";
    // Its own, for anyone.
    let (_, ours) = get(&h, lxm, &format!("handle={RIDERS}"), &[]).await;
    assert_eq!(ours["did"], riders.as_str());
    // Anyone else's, only for someone signed in: no stranger sends the host
    // to a handle's server.
    let (status, said) = get(&h, lxm, "handle=alice.example", &[]).await;
    assert_eq!(
        (status, said["error"].as_str()),
        (400, Some("HandleNotFound"))
    );
    let (status, said) = get(&h, lxm, "handle=alice.example", &[auth(&h, &alice, lxm)]).await;
    assert_eq!(
        (status, said["did"].as_str()),
        (200, Some(alice.did.as_str()))
    );
    let (status, _) = get(&h, lxm, "handle=nobody.example", &[auth(&h, &alice, lxm)]).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn checks_a_domain_before_it_is_taken() {
    let (h, names, did, [alice, _, _]) = here().await;
    names
        .0
        .lock()
        .unwrap()
        .insert("riders.test".into(), did.clone());
    names
        .0
        .lock()
        .unwrap()
        .insert("other.test".into(), "did:plc:other".into());
    assert_eq!(
        check(&h, &alice, &did, "@Riders.TEST").await,
        (
            200,
            json!({"handle": "riders.test", "ready": true, "did": did})
        )
    );
    assert_eq!(
        check(&h, &alice, &did, "other.test").await,
        (
            200,
            json!({"handle": "other.test", "ready": false, "did": "did:plc:other"})
        )
    );
    assert_eq!(
        check(&h, &alice, &did, "nobody.test").await,
        (200, json!({"handle": "nobody.test", "ready": false}))
    );
    // Its own address is always its own.
    assert_eq!(check(&h, &alice, &did, RIDERS).await.1["ready"], true);
}

#[tokio::test]
async fn takes_a_domain_that_names_it_and_its_address_back() {
    let (h, names, did, [alice, _, _]) = here().await;
    names
        .0
        .lock()
        .unwrap()
        .insert("riders.test".into(), did.clone());
    assert_eq!(
        update(&h, &alice, &did, "riders.test").await,
        (200, json!({"handle": "riders.test"}))
    );
    // The directory names it, then the host, then the relays hear.
    let ops = h.plc.ops.lock().unwrap()[&did].clone();
    let Some(Signed::Update { op, .. }) = ops.last() else {
        panic!("an update");
    };
    assert_eq!(op.also_known_as, ["at://riders.test"]);
    assert_eq!(
        h.host.store.account(&did).unwrap().unwrap().handle,
        "riders.test"
    );
    let events = h.host.store.events_after(0, 1000).unwrap();
    let last = atproto::drisl::decode(&events.last().unwrap().body)
        .unwrap()
        .to_json();
    assert_eq!(
        (events.last().unwrap().kind.as_str(), last["handle"].clone()),
        ("#identity", json!("riders.test"))
    );
    // Its address stays.
    let lxm = format!("{INTERNAL}.getGroup");
    let (_, shown) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(
        (
            shown["group"]["handle"].clone(),
            shown["group"]["address"].clone()
        ),
        (json!("riders.test"), json!(RIDERS))
    );

    // Asked again, nothing changes.
    let before = h.plc.ops.lock().unwrap()[&did].len();
    assert_eq!(update(&h, &alice, &did, "riders.test").await.0, 200);
    assert_eq!(h.plc.ops.lock().unwrap()[&did].len(), before);
    // Refused by the directory, the host keeps the handle the directory
    // names, and the relays hear nothing.
    let told = h.host.store.events_after(0, 1000).unwrap().len();
    turn(&h.plc.refuse, true);
    // A refusal is the caller's to send again: InvalidOperation.
    assert_eq!(update(&h, &alice, &did, RIDERS).await.0, 400);
    turn(&h.plc.refuse, false);
    assert_eq!(h.plc.ops.lock().unwrap()[&did].len(), before);
    assert_eq!(
        h.host.store.account(&did).unwrap().unwrap().handle,
        "riders.test"
    );
    assert_eq!(h.host.store.events_after(0, 1000).unwrap().len(), told);
    // And back to its address.
    assert_eq!(
        update(&h, &alice, &did, RIDERS).await,
        (200, json!({"handle": RIDERS}))
    );
    assert_eq!(h.host.store.account(&did).unwrap().unwrap().handle, RIDERS);
}

#[tokio::test]
async fn refuses_a_handle_it_cannot_take() {
    let (h, names, did, [alice, bob, _]) = here().await;
    names
        .0
        .lock()
        .unwrap()
        .insert("other.test".into(), "did:plc:other".into());
    // Another group here takes a domain first.
    let theirs = start_group(
        &h,
        &bob,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    names
        .0
        .lock()
        .unwrap()
        .insert("taken.test".into(), theirs.clone());
    assert_eq!(update(&h, &bob, &theirs, "taken.test").await.0, 200);

    for (handle, refused) in [
        ("not a handle", (400, "InvalidHandle")),
        // Under the host's domain, only its own address.
        ("hikers.groups.test", (400, "UnsupportedDomain")),
        ("groups.test", (400, "UnsupportedDomain")),
        ("taken.test", (400, "HandleNotAvailable")),
    ] {
        assert_eq!(
            error(&check(&h, &alice, &did, handle).await),
            refused,
            "{handle}"
        );
        // A handle that is none is refused by the update's lexicon.
        let refused = match refused.1 {
            "InvalidHandle" => (400, "InvalidRequest"),
            _ => refused,
        };
        assert_eq!(
            error(&update(&h, &alice, &did, handle).await),
            refused,
            "{handle}"
        );
    }
    assert_eq!(
        error(&update(&h, &alice, &did, "other.test").await),
        (400, "NotPointing")
    );
    assert_eq!(
        error(&update(&h, &alice, &did, "nobody.test").await),
        (400, "NotPointing")
    );
    // Only one who may configure it.
    assert_eq!(
        error(&check(&h, &bob, &did, "riders.test").await),
        (403, "Forbidden")
    );
    assert_eq!(
        error(&update(&h, &bob, &did, "riders.test").await),
        (403, "Forbidden")
    );
    let lxm = format!("{INTERNAL}.updateGroupHandle");
    for body in [json!({"handle": "riders.test"}), json!({"group": did})] {
        let said = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
        assert_eq!(error(&said), (400, "InvalidRequest"));
    }
    assert_eq!(
        error(&check(&h, &alice, "did:plc:nobody", "riders.test").await),
        (400, "GroupNotFound")
    );

    // Once the host holds no rotation key for it, it changes nothing.
    names
        .0
        .lock()
        .unwrap()
        .insert("riders.test".into(), did.clone());
    let elsewhere = atproto::Keypair::generate(atproto::Curve::K256);
    {
        let mut ops = h.plc.ops.lock().unwrap();
        let log = ops.get_mut(&did).unwrap();
        let last = log.last().cloned().unwrap();
        let Signed::Update { op, .. } = &last else {
            panic!("an update");
        };
        let mut moved = op.clone();
        moved.rotation_keys = vec![elsewhere.public().did_key()];
        log.push(moved.sign(Some(last.cid()), &h.host.secrets.rotation));
    }
    assert_eq!(
        error(&update(&h, &alice, &did, "riders.test").await),
        (400, "NotController")
    );
    // Nor once its DID is ended.
    {
        let mut ops = h.plc.ops.lock().unwrap();
        let log = ops.get_mut(&did).unwrap();
        let last = log.last().unwrap().cid();
        log.push(atproto::plc::tombstone(last, &elsewhere));
    }
    assert_eq!(
        error(&update(&h, &alice, &did, "riders.test").await),
        (400, "NotController")
    );
    // Nor when the directory cannot say.
    h.plc.ops.lock().unwrap().remove(&did);
    assert_eq!(update(&h, &alice, &did, "riders.test").await.0, 500);
}
