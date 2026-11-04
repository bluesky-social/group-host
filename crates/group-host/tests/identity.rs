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
