//! The firehose: every event after a cursor, then each as it is made, as
//! sync 1.1 frames them; and the relays told of news.

mod common;

use atproto::{Commit, PublicKey, Value, car, drisl};
use axum::Router;
use axum::routing::post;
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
        collection: "fyi.opensocial.rule".into(),
        rkey: rkey.into(),
        value: json!({"$type": "fyi.opensocial.rule", "title": "Ride safe", "createdAt": "2026-11-03T00:00:00Z"}),
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
        assert_eq!(text(field(&ops[0], "path")), "fyi.opensocial.rule/a");
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
                    collection: "fyi.opensocial.rule".into(),
                    rkey: "a".into(),
                    value: json!({"$type": "fyi.opensocial.rule", "title": "Ride safer", "createdAt": "2026-11-03T00:00:00Z"}),
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
                collection: "fyi.opensocial.rule".into(),
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
