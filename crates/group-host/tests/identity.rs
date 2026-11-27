//! Other accounts' DID documents, as the host reads them, and the hosts
//! identity data names, as the host asks them ([`group_host::guard`]).

mod common;

use axum::Router;
use axum::routing::get;
use common::serve;
use group_host::guard::{pinned_client, public};
use group_host::identity::Resolver;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A resolver as a host on http reads, on a network of test servers.
fn here() -> Resolver {
    Resolver::new("http://127.0.0.1:9", "http").http_here(true)
}

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
    let r = here();
    let err = |e: anyhow::Error| format!("{e:#}");
    for (did, why) in [
        ("did:key:zQ3sh", "did:key:zQ3sh: not a did:plc or a did:web"),
        ("did:web:", "not a did:web of a host: did:web:"),
        (
            "did:web:example.com:users:alice",
            "a did:web is of a host alone, with no path: did:web:example.com:users:alice",
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
async fn reads_a_did_web_where_it_is_told_to() {
    // A test server stands for grain.social: its document is read there.
    let at = webs(1).await.remove(0);
    let address = at.strip_prefix("did:web:").unwrap().replace("%3A", ":");
    let hosts = [("grain.social".to_string(), address)].into();
    let r = here().reading_at(&hosts);
    // The document served there names another DID, so it is refused as
    // another's: it was read from that server, not from grain.social.
    let refused = r.document("did:web:grain.social").await.unwrap_err();
    assert_eq!(
        refused.to_string(),
        "did:web:grain.social's document is another's"
    );
}

#[tokio::test]
async fn gives_no_key_where_a_document_names_none() {
    let r = here();
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
    let lapsing =
        Resolver::keeping("http://127.0.0.1:9", "http", Duration::ZERO, 2).http_here(true);
    for did in &dids {
        lapsing.document(did).await.unwrap();
    }
    assert_eq!(lapsing.kept(&dids[2]), None);
    // And with none lapsed, of all of them.
    let keeping =
        Resolver::keeping("http://127.0.0.1:9", "http", Duration::from_secs(60), 2).http_here(true);
    for did in &dids {
        keeping.document(did).await.unwrap();
    }
    assert_eq!(keeping.kept(&dids[0]), None);
    assert_eq!(keeping.kept(&dids[1]), None);
    assert!(keeping.kept(&dids[2]).is_some());
}

#[tokio::test]
async fn follows_no_redirect_and_reads_no_more_than_a_document() {
    let r = here();
    // A did:web or a handle whose host sends it elsewhere is not followed
    // there.
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let elsewhere = serve(Router::new().fallback(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        async { "did:plc:elsewhere" }
    }))
    .await;
    let redirect = move || {
        let to = format!("{elsewhere}/");
        async move { axum::response::Redirect::temporary(&to) }
    };
    let router = Router::new()
        .route("/.well-known/did.json", get(redirect.clone()))
        .route("/.well-known/atproto-did", get(redirect));
    let url = serve(router).await;
    let at = url.strip_prefix("http://").unwrap();
    let redirecting = format!("did:web:{}", at.replace(':', "%3A"));
    assert!(r.document(&redirecting).await.is_err());
    let well_known = ["well-known:http".to_string()];
    assert_eq!(r.resolve_handle(at, &well_known).await, None);
    assert_eq!(hits.load(Ordering::SeqCst), 0);

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

#[test]
fn knows_a_public_address() {
    for (ip, is) in [
        ("8.8.8.8", true),
        ("2606:4700::1111", true),
        ("::ffff:8.8.8.8", true),
        ("127.0.0.1", false),
        ("10.1.2.3", false),
        ("172.16.0.1", false),
        ("192.168.1.1", false),
        ("169.254.169.254", false),
        ("100.64.0.1", false),
        ("192.0.0.8", false),
        ("198.18.0.1", false),
        ("0.1.2.3", false),
        ("240.0.0.1", false),
        ("255.255.255.255", false),
        ("224.0.0.1", false),
        ("192.0.2.1", false),
        ("::1", false),
        ("::", false),
        ("::ffff:127.0.0.1", false),
        // An IPv4 address carried in IPv6 other than mapped, as NAT64 or
        // the old IPv4-compatible form carries one.
        ("64:ff9b::a00:1", false),
        ("::7f00:1", false),
        ("fd00::1", false),
        ("fec0::1", false),
        ("fe80::1", false),
        ("ff02::1", false),
        ("100::1", false),
        ("2001:db8::1", false),
    ] {
        assert_eq!(public(ip.parse().unwrap()), is, "{ip}");
    }
}

#[tokio::test]
async fn asks_a_host_others_name_only_on_https_at_a_public_address() {
    let refused = async |url: &str| pinned_client(url, false).await.unwrap_err();
    for url in ["https://8.8.8.8/xrpc", "https://[2606:4700::1111]/"] {
        let (_, at) = pinned_client(url, false).await.unwrap();
        assert_eq!(at.as_str(), url);
    }
    for url in [
        "https://127.0.0.1:2583",
        "https://[::1]",
        "https://10.0.0.2",
        "https://169.254.169.254/latest/meta-data",
        // A name pointing inward, as any name may.
        "https://localhost",
    ] {
        assert_eq!(
            refused(url).await,
            format!("{url} is not at a public address")
        );
    }
    assert_eq!(
        refused("http://8.8.8.8").await,
        "http://8.8.8.8 is not an https URL"
    );
    // On a network of test servers, on http too, at any address.
    assert!(pinned_client("http://127.0.0.1:2583", true).await.is_ok());
}

#[tokio::test]
async fn asks_no_did_web_or_handle_that_points_inward() {
    let err = |e: anyhow::Error| format!("{e:#}");
    // A did:web, and a handle, at a test server on this machine.
    let did = webs(1).await.remove(0);
    let at = did.strip_prefix("did:web:").unwrap().replace("%3A", ":");
    let handles = Router::new().route(
        "/.well-known/atproto-did",
        get(|| async { "did:plc:inward" }),
    );
    let handle = serve(handles).await;
    let handle = handle.strip_prefix("http://").unwrap();

    // A host on https reads neither: not at its address, nor by a name
    // for it, nor over http.
    let https = Resolver::new("http://127.0.0.1:9", "https");
    let port = at.rsplit(':').next().unwrap();
    for (did, url) in [
        (did.clone(), format!("https://{at}")),
        (
            format!("did:web:localhost%3A{port}"),
            format!("https://localhost:{port}"),
        ),
    ] {
        assert_eq!(
            err(https.document(&did).await.unwrap_err()),
            format!("{url}/.well-known/did.json is not at a public address")
        );
    }
    let http = Resolver::new("http://127.0.0.1:9", "http");
    assert_eq!(
        err(http.document(&did).await.unwrap_err()),
        format!("http://{at}/.well-known/did.json is not an https URL")
    );
    // Where it is told to read one instead is its own setting, read as
    // it is set.
    let hosts = [("grain.social".to_string(), at.clone())].into();
    let told = Resolver::new("http://127.0.0.1:9", "http").reading_at(&hosts);
    assert_eq!(
        err(told.document("did:web:grain.social").await.unwrap_err()),
        "did:web:grain.social's document is another's"
    );
    for scheme in ["https", "http"] {
        let well_known = [format!("well-known:{scheme}")];
        assert_eq!(https.resolve_handle(handle, &well_known).await, None);
    }

    // A host on http, on a network of test servers, reads them both.
    assert!(here().document(&did).await.is_ok());
    let well_known = ["well-known:http".to_string()];
    let named = here().resolve_handle(handle, &well_known).await;
    assert_eq!(named.as_deref(), Some("did:plc:inward"));
}
