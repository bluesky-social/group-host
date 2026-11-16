//! Other accounts' DID documents, as the host reads them.

mod common;

use axum::Router;
use axum::routing::get;
use common::serve;
use group_host::identity::Resolver;
use serde_json::{Value, json};
use std::time::Duration;

/// A server whose did:web document is `doc`, and its DID.
async fn web(doc: Value) -> String {
    let router = Router::new().route(
        "/.well-known/did.json",
        get(move || {
            let doc = doc.clone();
            async move { axum::Json(doc) }
        }),
    );
    let url = serve(router).await;
    format!(
        "did:web:{}",
        url.strip_prefix("http://").unwrap().replace(':', "%3A")
    )
}

#[tokio::test]
async fn reads_what_it_can_and_refuses_the_rest() {
    let r = Resolver::new("http://127.0.0.1:9", "http");
    let err = |e: anyhow::Error| format!("{e:#}");
    for (did, why) in [
        ("did:key:zQ3sh", "did:key:zQ3sh: not a did:plc or a did:web"),
        ("did:web:", "did:web: is not a did:web of a host"),
        (
            "did:web:example.com:users:alice",
            "did:web:example.com:users:alice is not a did:web of a host",
        ),
    ] {
        assert_eq!(err(r.document(did).await.unwrap_err()), why);
    }

    // A document of another DID is not this one's.
    let other = web(json!({"id": "did:web:elsewhere"})).await;
    assert_eq!(
        err(r.document(&other).await.unwrap_err()),
        format!("{other}'s document is another's")
    );
    assert_eq!(r.kept(&other), None);

    // Nor is one past the size a document may be.
    let padding = "x".repeat(group_host::identity::DOCUMENT_AT_MOST);
    let large = web(json!({"id": "did:web:large", "padding": padding})).await;
    assert_eq!(
        err(r.document(&large).await.unwrap_err()),
        format!(
            "a DID document past {} bytes",
            group_host::identity::DOCUMENT_AT_MOST
        )
    );
    // Nor one that is no object.
    let text = web(json!("not a document")).await;
    assert_eq!(
        err(r.document(&text).await.unwrap_err()),
        format!("{text}'s document is another's")
    );
}

#[tokio::test]
async fn gives_no_key_where_a_document_names_none() {
    let r = Resolver::new("http://127.0.0.1:9", "http");
    let web = webs(1).await.remove(0);
    let check = |_: &atproto::PublicKey| Ok::<(), ()>(());
    assert_eq!(r.with_key(&web, check).await, None);
    // Kept now, and still keyless when read again.
    assert!(r.kept(&web).is_some());
    assert_eq!(r.with_key(&web, check).await, None);
    // A directory that does not answer gives no document.
    assert!(r.with_key("did:plc:nowhere", check).await.is_none());
}

/// A server of `n` did:web documents, each its own DID's, by port.
async fn webs(n: usize) -> Vec<String> {
    let mut dids = Vec::new();
    for _ in 0..n {
        let did = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let served = did.clone();
        let router = Router::new().route(
            "/.well-known/did.json",
            get(move || {
                let id = served.lock().unwrap().clone();
                async move { axum::Json(json!({"id": id})) }
            }),
        );
        let url = serve(router).await;
        let web = format!(
            "did:web:{}",
            url.strip_prefix("http://").unwrap().replace(':', "%3A")
        );
        *did.lock().unwrap() = web.clone();
        dids.push(web);
    }
    dids
}

#[tokio::test]
async fn keeps_so_many_documents_and_no_more() {
    let dids = webs(3).await;
    // Full, it lets go of those lapsed first.
    let lapsing = Resolver::keeping("http://127.0.0.1:9", "http", Duration::ZERO, 2);
    for did in &dids {
        lapsing.document(did).await.unwrap();
    }
    assert_eq!(lapsing.kept(&dids[2]), None);
    // And with none lapsed, of all of them.
    let keeping = Resolver::keeping("http://127.0.0.1:9", "http", Duration::from_secs(60), 2);
    for did in &dids {
        keeping.document(did).await.unwrap();
    }
    assert_eq!(keeping.kept(&dids[0]), None);
    assert_eq!(keeping.kept(&dids[1]), None);
    assert!(keeping.kept(&dids[2]).is_some());
}

#[tokio::test]
async fn follows_no_redirect_and_reads_no_more_than_a_document() {
    let r = Resolver::new("http://127.0.0.1:9", "http");
    // A did:web whose host sends it elsewhere is not followed there.
    let target = webs(1).await.remove(0);
    let target_url = format!(
        "http://{}/.well-known/did.json",
        target.strip_prefix("did:web:").unwrap().replace("%3A", ":")
    );
    let router = Router::new().route(
        "/.well-known/did.json",
        get(move || {
            let to = target_url.clone();
            async move { axum::response::Redirect::temporary(&to) }
        }),
    );
    let url = serve(router).await;
    let redirecting = format!(
        "did:web:{}",
        url.strip_prefix("http://").unwrap().replace(':', "%3A")
    );
    assert!(r.document(&redirecting).await.is_err());

    // A resolver that answers more than a document may be names nobody.
    let big = "x".repeat(group_host::identity::DOCUMENT_AT_MOST + 1);
    let router = Router::new().route(
        "/xrpc/com.atproto.identity.resolveHandle",
        get(move || {
            let big = big.clone();
            async move { axum::Json(json!({"did": "did:plc:big", "padding": big})) }
        }),
    );
    let service = serve(router).await;
    assert_eq!(r.resolve_handle("riders.test", &[service]).await, None);
}

#[test]
fn gives_an_account_made_before_addresses_its_handle_as_its_address() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.sqlite");
    let store = group_host::store::Store::open(&path).unwrap();
    let account = group_host::store::Account {
        did: "did:plc:a".into(),
        handle: "a.groups.test".into(),
        signing: atproto::Keypair::generate(atproto::Curve::K256),
        kem: atproto::Keypair::generate(atproto::Curve::P256),
    };
    let tree = atproto::mst::Tree::build(&Default::default());
    let commit =
        atproto::Commit::sign(&account.did, tree.root(), "3l5mz5mkbn22b", &account.signing);
    let head = group_host::store::Head {
        cid: commit.cid(),
        rev: commit.rev.clone(),
        block: commit.to_bytes(),
    };
    store.create_account(&account, &head, 0, &[]).unwrap();
    drop(store);
    // As a database from before: no address, its schema a version back.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("DELETE FROM address", []).unwrap();
    // Back to just before the backfill.
    let backfill = group_host::store::MIGRATIONS
        .iter()
        .position(|m| m.contains("INSERT OR IGNORE INTO address"))
        .unwrap();
    common::rewind(&conn, backfill);
    drop(conn);
    let store = group_host::store::Store::open(&path).unwrap();
    assert_eq!(
        store.address("did:plc:a").unwrap().as_deref(),
        Some("a.groups.test")
    );
}
