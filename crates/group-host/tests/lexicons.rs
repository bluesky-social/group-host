//! Every `org.atgrp.*` method's parameters and input, checked against
//! its lexicon before its handler runs.

mod common;

use common::start;
use group_host::lexicons::{FILES, INPUT_AT_MOST, lexicons};
use serde_json::Value as Json;

/// A POST of `body`, as it is, to `nsid`.
async fn send(h: &common::Running, nsid: &str, body: Vec<u8>) -> (u16, Json) {
    let res = h
        .http
        .post(format!("{}/xrpc/{nsid}", h.url))
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();
    (
        res.status().as_u16(),
        res.json().await.unwrap_or(Json::Null),
    )
}

#[tokio::test]
async fn refuses_what_a_lexicon_does_not_describe_before_its_handler() {
    let h = start(&[], false).await;
    // Nobody signed in: the refusals come before the handler would ask.
    let said = send(
        &h,
        "org.atgrp.internal.deactivateGroup",
        b"{\"group\": 1}".to_vec(),
    )
    .await;
    assert_eq!(said.0, 400);
    assert_eq!(said.1["error"], "InvalidRequest");
    assert_eq!(said.1["message"], "input.group must be a string");
    let said = send(
        &h,
        "org.atgrp.internal.deactivateGroup",
        b"not json".to_vec(),
    )
    .await;
    assert_eq!(said.1["message"], "the input is not JSON");
    let said = send(
        &h,
        "org.atgrp.internal.deactivateGroup",
        vec![b' '; INPUT_AT_MOST + 1],
    )
    .await;
    assert_eq!(
        (said.0, said.1["error"].as_str()),
        (413, Some("PayloadTooLarge"))
    );
    let (status, said) = h
        .get("org.atgrp.internal.getGroup", "group=not-a-did")
        .await;
    assert_eq!(status, 400, "{said}");
    let (status, said) = h.get("org.atgrp.internal.getGroup", "").await;
    assert_eq!(
        (status, said["message"].as_str()),
        (400, Some("params must have the property \"group\""))
    );
    // What the lexicon lets through reaches the handler.
    let (status, said) = h
        .get("org.atgrp.internal.getGroup", "group=did:plc:nothing")
        .await;
    assert_eq!(
        (status, said["error"].as_str()),
        (400, Some("GroupNotFound"))
    );
    // A method asked the wrong way is the router's to refuse.
    assert_eq!(h.get("org.atgrp.internal.deactivateGroup", "").await.0, 405);
    // A method with no lexicon here is not looked at.
    let (status, _) = h.get("org.atgrp.noSuchMethod", "").await;
    assert_eq!(status, 501);
}

#[test]
fn holds_every_lexicon() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lexicons");
    let mut found = 0;
    let mut dirs = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                found += 1;
                let doc: Json =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                let id = doc["id"].as_str().unwrap();
                assert!(
                    lexicons()
                        .def(&format!("{id}#{}", first_def(&doc)))
                        .is_some(),
                    "{id}"
                );
            }
        }
    }
    assert_eq!(found, FILES.len());
    assert!(found > 50);
}

/// The name of a document's first def.
fn first_def(doc: &Json) -> String {
    doc["defs"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone()
}
