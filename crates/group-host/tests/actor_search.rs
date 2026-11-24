//! Finding an account by what is typed of its handle: in the directory's
//! own handles on a network of its own, or from a typeahead service.

mod common;

use axum::Router;
use axum::extract::Query;
use axum::routing::get;
use common::groups::*;
use common::{Running, serve, start};
use serde_json::{Value as Json, json};
use std::collections::HashMap;

async fn search(h: &Running, query: &str) -> Vec<Json> {
    let (status, said) = h.get("org.atgrp.internal.searchActors", query).await;
    assert_eq!(status, 200, "{said}");
    said["actors"].as_array().unwrap().clone()
}

fn handles(found: &[Json]) -> Vec<&str> {
    found
        .iter()
        .map(|a| a["handle"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn on_a_network_of_its_own_the_directory_is_searched() {
    // The test directory is not plc.directory, so the host reads its export.
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    for (handle, name) in [
        ("riders", "Riders"),
        ("ride-share", "Share"),
        ("bakers", "Bakers"),
    ] {
        let body = json!({"handle": format!("{handle}.groups.test"), "displayName": name});
        start_group(&h, &alice, body).await;
    }
    // A group ended is no one to find: its DID's last operation is a tombstone.
    let gone = start_group(
        &h,
        &alice,
        json!({"handle": "riders-gone.groups.test", "displayName": "Gone"}),
    )
    .await;
    let lxm = format!("{INTERNAL}.deleteGroup");
    let (status, said) = post(&h, &lxm, &json!({"group": gone}), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(
        (status, said["tombstoned"].clone()),
        (200, json!(true)),
        "{said}"
    );

    let found = search(&h, "q=rid").await;
    assert_eq!(
        handles(&found),
        ["riders.groups.test", "ride-share.groups.test"]
    );
    assert!(found[0]["did"].as_str().unwrap().starts_with("did:plc:"));
    assert!(found[0].get("displayName").is_none());
    // Beginning with it first, then holding it; typed with an @, in capitals.
    assert_eq!(
        handles(&search(&h, "q=%40AKERS").await),
        ["bakers.groups.test"]
    );
    assert_eq!(handles(&search(&h, "q=groups&limit=1").await).len(), 1);
    assert!(search(&h, "q=r").await.is_empty());
    assert!(search(&h, "q=nobody").await.is_empty());

    // Kept a minute: a group started since is not found until then.
    start_group(
        &h,
        &alice,
        json!({"handle": "rivers.groups.test", "displayName": "Rivers"}),
    )
    .await;
    assert!(search(&h, "q=rivers").await.is_empty());
}

#[tokio::test]
async fn a_directory_that_cannot_be_read_finds_no_one() {
    let away = [("PLC_URL", "http://127.0.0.1:9"), ("ACTOR_SEARCH", "plc")];
    let h = start(&away, false).await;
    assert!(search(&h, "q=riders").await.is_empty());
}

#[tokio::test]
async fn searches_at_once_share_one_read_of_the_directory() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let body = json!({"handle": "riders.groups.test", "displayName": "Riders"});
    start_group(&h, &alice, body).await;
    let asked = (0..8).map(|_| search(&h, "q=riders"));
    for found in futures::future::join_all(asked).await {
        assert_eq!(handles(&found), ["riders.groups.test"]);
    }
    // One read: a page of two operations at a time, then the empty one.
    let operations = h.plc.exported.lock().unwrap().len();
    let read = h.plc.exports.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(read, operations.div_ceil(2) + 1);
    search(&h, "q=riders").await;
    assert_eq!(
        h.plc.exports.load(std::sync::atomic::Ordering::SeqCst),
        read
    );
}

#[tokio::test]
async fn on_the_whole_network_the_typeahead_service_is_asked() {
    let service = Router::new().route(
        "/search",
        get(|Query(q): Query<HashMap<String, String>>| async move {
            axum::Json(json!({"actors": [
                {"did": "did:plc:alice", "handle": "alice.test", "displayName": "Alice", "avatar": "https://cdn.test/a.jpg"},
                {"did": "did:plc:alfie", "handle": format!("{}.test", q["q"])},
                {"did": "did:plc:broken", "handle": "handle.invalid"},
                {"handle": "nodid.test"},
                {"did": "did:plc:limit", "handle": format!("limit-{}.test", q["limit"])},
            ]}))
        }),
    );
    let url = format!("{}/search", serve(service).await);
    let h = start(&[("ACTOR_SEARCH", &url)], false).await;
    let found = search(&h, "q=al&limit=3").await;
    assert_eq!(handles(&found), ["alice.test", "al.test", "limit-3.test"]);
    assert_eq!(found[0]["displayName"], "Alice");
    assert_eq!(found[0]["avatar"], "https://cdn.test/a.jpg");
    assert!(found[1].get("avatar").is_none());

    let away = start(&[("ACTOR_SEARCH", "http://127.0.0.1:9/search")], false).await;
    assert!(search(&away, "q=alice").await.is_empty());
}
