//! Recovery keys: a steward's rotation key ahead of the host's, so the
//! group's identity is never the host's alone; and a deleted group's DID
//! left to whoever holds one.

mod common;

use atproto::plc::{Signed, tombstone};
use atproto::{Curve, Keypair};
use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use serde_json::{Value as Json, json};

async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    (did, [alice, bob, carol])
}

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn list(h: &Running, who: &Authority, did: &str) -> (u16, Json) {
    let lxm = format!("{NS}.listRecoveryKeys");
    get(h, &lxm, &format!("group={did}"), &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// The group's rotation keys, as the directory holds them.
async fn rotation_keys(h: &Running, did: &str) -> Vec<String> {
    match h.host.plc.last_op(did).await.unwrap() {
        Signed::Update { op, .. } => op.rotation_keys,
        Signed::Tombstone { .. } => panic!("ended"),
    }
}

fn key() -> (Keypair, String) {
    let k = Keypair::generate(Curve::K256);
    let did = k.public().did_key();
    (k, did)
}

#[tokio::test]
async fn puts_a_stewards_key_ahead_of_the_hosts() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let host_key = h.host.secrets.rotation.public().did_key();
    let (_, first) = key();
    let (_, second) = key();
    for k in [&first, &second] {
        let said = call(
            &h,
            &alice,
            "addRecoveryKey",
            json!({"group": did, "key": k}),
        )
        .await;
        assert_eq!(said.0, 200, "{}", said.1);
    }
    // Earlier keys keep their rank; the host's is last.
    assert_eq!(
        rotation_keys(&h, &did).await,
        [first.clone(), second.clone(), host_key.clone()]
    );
    let (status, listed) = list(&h, &alice, &did).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["hostHolds"], true);
    assert_eq!(listed["max"], 4);
    let keys = listed["keys"].as_array().unwrap();
    assert_eq!(keys[0]["key"], first.as_str());
    assert_eq!(keys[0]["holder"], "steward");
    assert_eq!(keys[0]["addedBy"], alice.did.as_str());
    assert!(keys[0]["addedAt"].is_string());
    assert_eq!(keys[2], json!({"key": host_key, "holder": "host"}));

    // Taken off again, the host's kept.
    let said = call(
        &h,
        &alice,
        "removeRecoveryKey",
        json!({"group": did, "key": first}),
    )
    .await;
    assert_eq!(said.0, 200, "{}", said.1);
    assert_eq!(rotation_keys(&h, &did).await, [second, host_key]);
    let (_, grants) = get(
        &h,
        &format!("{NS}.listGroupTokens"),
        &format!("group={did}"),
        &[auth(&h, &alice, &format!("{NS}.listGroupTokens"))],
    )
    .await;
    let actions: Vec<&str> = grants["activity"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions[..2],
        ["removed a recovery key", "added a recovery key"]
    );
}

#[tokio::test]
async fn refuses_a_key_it_should_not_add_or_take_off() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let host_key = h.host.secrets.rotation.public().did_key();
    let keys: Vec<String> = (0..4).map(|_| key().1).collect();
    for k in &keys {
        assert_eq!(
            call(
                &h,
                &alice,
                "addRecoveryKey",
                json!({"group": did, "key": k})
            )
            .await
            .0,
            200
        );
    }
    let add = async |who: &Authority, body: Json| call(&h, who, "addRecoveryKey", body).await;
    for (body, refused) in [
        (
            json!({"group": did, "key": "not a key"}),
            (400, "InvalidKey"),
        ),
        (json!({"group": did, "key": keys[0]}), (400, "AlreadyAdded")),
        (
            json!({"group": did, "key": host_key}),
            (400, "AlreadyAdded"),
        ),
        (json!({"group": did, "key": key().1}), (400, "TooManyKeys")),
        (json!({"key": key().1}), (400, "InvalidRequest")),
        (
            json!({"group": "did:plc:nothing", "key": key().1}),
            (400, "GroupNotFound"),
        ),
    ] {
        assert_eq!(error(&add(&alice, body.clone()).await), refused, "{body}");
    }
    assert_eq!(
        error(&add(&bob, json!({"group": did, "key": key().1})).await),
        (403, "Forbidden")
    );
    assert_eq!(list(&h, &bob, &did).await.0, 403);
    let remove = async |body: Json| call(&h, &alice, "removeRecoveryKey", body).await;
    assert_eq!(
        error(&remove(json!({"group": did, "key": host_key})).await),
        (400, "HostKey")
    );
    assert_eq!(
        error(&remove(json!({"group": did, "key": key().1})).await),
        (400, "UnknownKey")
    );
}

#[tokio::test]
async fn changes_nothing_once_the_host_holds_no_key() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let (steward, theirs) = key();
    assert_eq!(
        call(
            &h,
            &alice,
            "addRecoveryKey",
            json!({"group": did, "key": theirs})
        )
        .await
        .0,
        200
    );
    // The steward takes the identity, leaving the host out, as they may.
    let Signed::Update { op, .. } = h.host.plc.last_op(&did).await.unwrap() else {
        panic!()
    };
    let last = h.host.plc.last_op(&did).await.unwrap();
    let mut next = op.clone();
    next.rotation_keys = vec![theirs.clone()];
    h.host
        .plc
        .publish(&did, &next.sign(Some(last.cid()), &steward))
        .await
        .unwrap();

    let (_, listed) = list(&h, &alice, &did).await;
    assert_eq!(listed["hostHolds"], false);
    assert_eq!(
        listed["keys"],
        json!([{"key": theirs, "holder": "steward", "addedBy": alice.did, "addedAt": listed["keys"][0]["addedAt"]}])
    );
    let said = call(
        &h,
        &alice,
        "addRecoveryKey",
        json!({"group": did, "key": key().1}),
    )
    .await;
    assert_eq!(error(&said), (400, "NotController"));
    let said = call(
        &h,
        &alice,
        "removeRecoveryKey",
        json!({"group": did, "key": theirs}),
    )
    .await;
    assert_eq!(error(&said), (400, "NotController"));
}

#[tokio::test]
async fn says_so_of_an_ended_did() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let last = h.host.plc.last_op(&did).await.unwrap();
    h.host
        .plc
        .publish(&did, &tombstone(last.cid(), &h.host.secrets.rotation))
        .await
        .unwrap();
    assert_eq!(error(&list(&h, &alice, &did).await), (400, "NotController"));
}

#[tokio::test]
async fn leaves_a_deleted_groups_did_to_a_steward_who_holds_a_key() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let (_, theirs) = key();
    assert_eq!(
        call(
            &h,
            &alice,
            "addRecoveryKey",
            json!({"group": did, "key": theirs})
        )
        .await
        .0,
        200
    );
    let (status, deleted) = call(&h, &alice, "deleteGroup", json!({"group": did})).await;
    assert_eq!(status, 200, "{deleted}");
    assert_eq!(deleted["tombstoned"], false);
    // Still theirs to bring back elsewhere.
    assert!(rotation_keys(&h, &did).await.contains(&theirs));
}
