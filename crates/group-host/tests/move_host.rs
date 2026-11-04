//! The host moved to a new address: each account's DID document pointed
//! there, and nothing it may not change.

mod common;

use atproto::plc::{Signed, tombstone};
use atproto::{Curve, Keypair};
use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use group_host::Host;
use group_host::config::Config;
use group_host::move_host::move_host;
use serde_json::json;
use std::collections::HashMap;

/// The host `h` is, opened again as its environment names `url`.
fn at(h: &Running, url: &str) -> Host {
    let env: HashMap<&str, String> = HashMap::from([
        ("DATA_DIR", h.data.path().to_str().unwrap().to_string()),
        ("PLC_URL", h.host.config.plc_url.clone()),
        ("HANDLE_DOMAIN", ".groups.test".to_string()),
        ("PUBLIC_URL", url.to_string()),
    ]);
    Host::open(Config::from(|k| env.get(k).cloned()).unwrap()).unwrap()
}

async fn pds(h: &Running, did: &str) -> String {
    let Signed::Update { op, .. } = h.host.plc.last_op(did).await.unwrap() else {
        panic!("ended")
    };
    op.services["atproto_pds"].endpoint.clone()
}

async fn group(h: &Running, handle: &str) -> String {
    let [alice, ..] = people(h).await;
    start_group(
        h,
        &alice,
        json!({"handle": handle, "displayName": "A group"}),
    )
    .await
}

#[tokio::test]
async fn points_each_accounts_document_at_the_new_address() {
    let h = start(&[], false).await;
    let riders = group(&h, RIDERS).await;
    let moved = at(&h, "https://new.test");

    // A dry run says what it would do, and does nothing.
    let said = move_host(&moved, "https://host.test/", true).await.unwrap();
    assert!(said[0].contains("a dry run"), "{said:?}");
    assert_eq!(said.last().unwrap(), "1 of 1 DID documents to repoint");
    assert_eq!(pds(&h, &riders).await, "https://host.test");

    let said = move_host(&moved, "https://host.test", false).await.unwrap();
    assert_eq!(
        said[1..],
        [
            format!("{RIDERS}: atproto_pds → https://new.test"),
            "1 of 1 DID documents repointed".to_string()
        ]
    );
    assert_eq!(pds(&h, &riders).await, "https://new.test");
    // Again, nothing is left to repoint.
    let said = move_host(&moved, "https://host.test", false).await.unwrap();
    assert_eq!(said.last().unwrap(), "0 of 1 DID documents repointed");
    let said = move_host(&moved, "https://host.test", true).await.unwrap();
    assert_eq!(said.last().unwrap(), "0 of 1 DID documents to repoint");
}

#[tokio::test]
async fn skips_an_ended_did_and_one_a_steward_took() {
    let h = start(&[], false).await;
    let ended = group(&h, "ended.groups.test").await;
    let last = h.host.plc.last_op(&ended).await.unwrap();
    h.host
        .plc
        .publish(&ended, &tombstone(last.cid(), &h.host.secrets.rotation))
        .await
        .unwrap();
    let taken = group(&h, "taken.groups.test").await;
    let steward = Keypair::generate(Curve::K256);
    let Signed::Update { op, .. } = h.host.plc.last_op(&taken).await.unwrap() else {
        panic!()
    };
    let last = h.host.plc.last_op(&taken).await.unwrap();
    let mut next = op.clone();
    next.rotation_keys = vec![steward.public().did_key()];
    h.host
        .plc
        .publish(
            &taken,
            &next.sign(Some(last.cid()), &h.host.secrets.rotation),
        )
        .await
        .unwrap();
    let said = move_host(&at(&h, "https://new.test"), "https://host.test", false)
        .await
        .unwrap();
    assert!(
        said.contains(&"ended.groups.test: its DID is ended, skipped".to_string()),
        "{said:?}"
    );
    assert!(said.contains(
        &"taken.groups.test: this host holds no rotation key to it, skipped".to_string()
    ));
    assert_eq!(said.last().unwrap(), "0 of 2 DID documents repointed");
}

#[tokio::test]
async fn refuses_to_move_to_where_it_is() {
    let h = start(&[], false).await;
    let said = move_host(&h.host, "https://host.test/", false).await;
    assert!(
        said.unwrap_err()
            .to_string()
            .contains("still names https://host.test")
    );
}
