//! The firehose: every event after a cursor, then each as it is made, as
//! sync 1.1 frames them; and the relays told of news.

mod common;

use atproto::{Commit, PublicKey, Value, car, drisl};
use axum::Router;
use axum::routing::post;
use common::groups::{people, start_group};
use common::{Running, start};
use futures::{SinkExt, StreamExt};
use group_host::repo::Write;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

const RIDERS: &str = "riders.groups.test";

fn create(rkey: &str) -> Write {
    Write::Create {
        collection: "org.atgrp.rule".into(),
        rkey: rkey.into(),
        value: json!({"$type": "org.atgrp.rule", "title": "Ride safe", "text": "Helmets on."}),
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(h: &Running, query: &str) -> Socket {
    let url = format!(
        "{}/xrpc/com.atproto.sync.subscribeRepos{query}",
        h.url.replace("http", "ws")
    );
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

/// The next frame: its header and body.
async fn next(socket: &mut Socket) -> (Value, Value) {
    let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = message else {
        panic!("{message:?}")
    };
    let (header, read) = drisl::decode_first(&bytes).unwrap();
    (header, drisl::decode(&bytes[read..]).unwrap())
}

fn field<'a>(v: &'a Value, k: &str) -> &'a Value {
    let Value::Map(m) = v else { panic!("{v:?}") };
    &m[k]
}

fn text(v: &Value) -> &str {
    let Value::String(s) = v else { panic!("{v:?}") };
    s
}

#[tokio::test]
async fn sends_every_event_after_a_cursor() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    h.host.write(&did, &[create("a")]).unwrap();
    let mut socket = connect(&h, "?cursor=0").await;

    let kinds: Vec<(String, i64)> = {
        let mut seen = Vec::new();
        let mut bodies = Vec::new();
        for _ in 0..4 {
            let (header, body) = next(&mut socket).await;
            assert_eq!(field(&header, "op"), &Value::Integer(1));
            let Value::Integer(seq) = field(&body, "seq") else {
                panic!()
            };
            seen.push((text(field(&header, "t")).to_string(), *seq));
            bodies.push(body);
        }
        // The account's first commit: no ops, nothing before it.
        let first = &bodies[2];
        assert_eq!(field(first, "since"), &Value::Null);
        assert_eq!(field(first, "ops"), &Value::Array(vec![]));
        assert_eq!(text(field(&bodies[0], "handle")), RIDERS);
        assert_eq!(field(&bodies[1], "active"), &Value::Bool(true));

        // The write: its op, its rev after the first's, and a CAR slice
        // holding the commit, signed by the account, the new root and the
        // new record.
        let write = &bodies[3];
        assert_eq!(text(field(write, "since")), text(field(first, "rev")));
        let Value::Array(ops) = field(write, "ops") else {
            panic!()
        };
        assert_eq!(text(field(&ops[0], "action")), "create");
        assert_eq!(text(field(&ops[0], "path")), "org.atgrp.rule/a");
        let Value::Bytes(blocks) = field(write, "blocks") else {
            panic!()
        };
        let car = car::read(blocks).unwrap();
        let Value::Link(cid) = field(write, "commit") else {
            panic!()
        };
        assert_eq!(car.roots, vec![*cid]);
        let commit = Commit::from_bytes(&car.blocks[cid]).unwrap();
        let account = h.host.store.account(&did).unwrap().unwrap();
        let key = PublicKey::from_did_key(&account.signing.public().did_key()).unwrap();
        assert!(commit.verify(&key));
        assert!(car.blocks.contains_key(&commit.data));
        let Value::Link(record) = field(&ops[0], "cid") else {
            panic!()
        };
        assert!(car.blocks.contains_key(record));
        let previous = Commit::from_bytes(
            &car::read(match field(first, "blocks") {
                Value::Bytes(b) => b,
                _ => panic!(),
            })
            .unwrap()
            .blocks[match field(first, "commit") {
                Value::Link(c) => c,
                _ => panic!(),
            }],
        )
        .unwrap();
        assert_eq!(field(write, "prevData"), &Value::Link(previous.data));
        seen
    };
    assert_eq!(
        kinds,
        [
            ("#identity".into(), 1),
            ("#account".into(), 2),
            ("#commit".into(), 3),
            ("#commit".into(), 4)
        ]
    );

    // Then each as it is made: an update and a delete, each with its prev.
    h.host
        .write(
            &did,
            &[
                Write::Update {
                    collection: "org.atgrp.rule".into(),
                    rkey: "a".into(),
                    value: json!({"$type": "org.atgrp.rule", "title": "Ride safer", "text": "Helmets on."}),
                },
            ],
        )
        .unwrap();
    let (_, update) = next(&mut socket).await;
    let Value::Array(ops) = field(&update, "ops") else {
        panic!()
    };
    assert_eq!(text(field(&ops[0], "action")), "update");
    assert!(matches!(field(&ops[0], "prev"), Value::Link(_)));
    h.host
        .write(
            &did,
            &[Write::Delete {
                collection: "org.atgrp.rule".into(),
                rkey: "a".into(),
            }],
        )
        .unwrap();
    let (_, delete) = next(&mut socket).await;
    let Value::Array(ops) = field(&delete, "ops") else {
        panic!()
    };
    assert_eq!(text(field(&ops[0], "action")), "delete");
    assert_eq!(field(&ops[0], "cid"), &Value::Null);

    // A message from the client changes nothing; closing ends the stream.
    socket.send(Message::Text("hello".into())).await.unwrap();
    socket.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn follows_from_now_without_a_cursor() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let mut socket = connect(&h, "").await;
    h.host.write(&did, &[create("a")]).unwrap();
    let (header, body) = next(&mut socket).await;
    assert_eq!(text(field(&header, "t")), "#commit");
    assert_eq!(field(&body, "seq"), &Value::Integer(4));
}

/// What atproto's own consumer (`harness/firehose.mjs`, @atproto/sync's
/// Firehose) made of every event of `h`'s firehose, reading as `mode`
/// says (`authenticated`, as an AppView, each commit checked against the
/// signing key of the account `dids` names; `unauthenticated`, as a
/// Jetstream): a line of JSON for each.
async fn as_sync_reads(h: &Running, dids: &[&str], mode: &'static str) -> Vec<serde_json::Value> {
    let keys: Vec<String> = dids
        .iter()
        .map(|did| {
            let account = h.host.store.account(did).unwrap().unwrap();
            format!("{did}={}", account.signing.public().did_key())
        })
        .collect();
    let url = h.url.clone();
    tokio::task::spawn_blocking(move || node_reads(&url, mode, &keys))
        .await
        .unwrap()
}

fn node_reads(url: &str, mode: &str, keys: &[String]) -> Vec<serde_json::Value> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new("node")
        .arg(root.join("harness/firehose.mjs"))
        .args([url, mode])
        .args(keys)
        .output()
        .expect("node runs harness/firehose.mjs (npm ci in harness/)");
    let lines = String::from_utf8(out.stdout).unwrap();
    lines
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The ops among what `harness/firehose.mjs` made out, each as its
/// action and at-uri; refused, for any event it could not read.
fn ops_of(read: &[serde_json::Value]) -> Vec<(String, String)> {
    let errors: Vec<_> = read.iter().filter(|e| e.get("error").is_some()).collect();
    assert!(errors.is_empty(), "{errors:#?}");
    read.iter()
        .filter(|e| e["kind"] == "commit")
        .map(|e| {
            (
                e["op"].as_str().unwrap().into(),
                e["uri"].as_str().unwrap().into(),
            )
        })
        .collect()
}

#[tokio::test]
async fn reads_as_atprotos_own_consumer_reads_it() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    // Enough records for a tree of more than one level, so each op's
    // proof is a path through it; then one changed, and one taken out.
    for i in 0..40 {
        h.host.write(&did, &[create(&format!("r{i:02}"))]).unwrap();
    }
    let rule = json!({"$type": "org.atgrp.rule", "title": "Ride safer", "text": "Helmets on."});
    let (collection, rkey) = ("org.atgrp.rule".to_string(), "r17".to_string());
    let update = Write::Update {
        collection: collection.clone(),
        rkey: rkey.clone(),
        value: rule,
    };
    h.host.write(&did, &[update]).unwrap();
    h.host
        .write(&did, &[Write::Delete { collection, rkey }])
        .unwrap();

    let uri = |rkey: &str| format!("at://{did}/org.atgrp.rule/{rkey}");
    for mode in ["authenticated", "unauthenticated"] {
        let read = as_sync_reads(&h, &[&did], mode).await;
        // The account's first commit has no ops, so it makes no event.
        assert_eq!(
            (&read[0]["kind"], &read[1]["kind"]),
            (&json!("identity"), &json!("account"))
        );
        let ops = ops_of(&read);
        assert_eq!(ops.len(), 42, "{mode}");
        assert_eq!(ops[0], ("create".into(), uri("r00")));
        assert_eq!(ops[40], ("update".into(), uri("r17")));
        assert_eq!(ops[41], ("delete".into(), uri("r17")));
    }
}

#[tokio::test]
async fn a_groups_events_read_as_a_jetstream_reads_them() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let body = json!({"handle": RIDERS, "displayName": "Riders", "metaPublicRead": true});
    let group = start_group(&h, &alice, body).await;
    for mode in ["authenticated", "unauthenticated"] {
        let ops = ops_of(&as_sync_reads(&h, &[&group], mode).await);
        assert!(
            ops.iter()
                .any(|(_, uri)| uri.contains("/org.atgrp.declaration/")),
            "{mode}: {ops:?}"
        );
    }
}

#[tokio::test]
async fn pages_through_a_long_history() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    for i in 0..510 {
        h.host.write(&did, &[create(&format!("r{i:03}"))]).unwrap();
    }
    let mut socket = connect(&h, "?cursor=0").await;
    for seq in 1..=513 {
        let (_, body) = next(&mut socket).await;
        assert_eq!(field(&body, "seq"), &Value::Integer(seq));
    }
}

#[tokio::test]
async fn refuses_a_cursor_it_cannot_follow() {
    let h = start(&[], false).await;
    let mut socket = connect(&h, "?cursor=99").await;
    let (header, body) = next(&mut socket).await;
    assert_eq!(field(&header, "op"), &Value::Integer(-1));
    assert_eq!(text(field(&body, "error")), "FutureCursor");

    let url = format!(
        "{}/xrpc/com.atproto.sync.subscribeRepos?cursor=x",
        h.url.replace("http", "ws")
    );
    let refused = tokio_tungstenite::connect_async(url).await.unwrap_err();
    assert!(refused.to_string().contains("400"), "{refused}");
}

#[tokio::test]
async fn tells_the_relays_of_news() {
    let asked: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let seen = asked.clone();
    let relay = Router::new().route(
        "/xrpc/com.atproto.sync.requestCrawl",
        post(
            move |axum::Json(body): axum::Json<serde_json::Value>| async move {
                seen.lock().unwrap().push(body);
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, relay).await.unwrap() });

    // One relay that answers, one that is not there, and a blank.
    let crawlers = format!("{relay_url}/, http://127.0.0.1:1, ");
    let h = start(&[("PDS_CRAWLERS", crawlers.as_str())], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    h.host.write(&did, &[create("a")]).unwrap();
    for _ in 0..50 {
        if !asked.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    // Told once, though two things happened, as twenty minutes have not passed.
    assert_eq!(
        *asked.lock().unwrap(),
        vec![json!({"hostname": "host.test"})]
    );
    assert_eq!(
        h.host.config.crawlers,
        vec![relay_url, "http://127.0.0.1:1".to_string()]
    );
}
