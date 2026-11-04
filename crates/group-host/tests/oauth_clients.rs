//! OAuth clients: their metadata documents, checked as atproto has them;
//! the loopback client; and a confidential client, which signs each
//! request to the server with a key its metadata lists.

mod common;

use atproto::jose;
use atproto::{Curve, Keypair};
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::get;
use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, serve, start, start_reachable};
use serde_json::{Value as Json, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A change to a client's metadata.
type Change = Box<dyn Fn(&mut Json)>;

type Docs = Arc<Mutex<HashMap<String, (u16, String)>>>;

async fn doc(
    State(docs): State<Docs>,
    Path(name): Path<String>,
) -> (axum::http::StatusCode, String) {
    let (status, body) = docs
        .lock()
        .unwrap()
        .get(&name)
        .cloned()
        .unwrap_or((404, String::new()));
    (axum::http::StatusCode::from_u16(status).unwrap(), body)
}

/// A server of client metadata documents, by name.
async fn documents() -> (Docs, String) {
    let docs: Docs = Arc::default();
    let url = serve(
        Router::new()
            .route("/{name}", get(doc))
            .with_state(docs.clone()),
    )
    .await;
    (docs, url)
}

/// A confidential client's metadata, at `id`, with these keys.
fn confidential(id: &str, keys: &[(&str, &Keypair)]) -> Json {
    let keys: Vec<Json> = keys
        .iter()
        .map(|(kid, k)| {
            let mut jwk = jose::jwk(&k.public());
            jwk["kid"] = json!(kid);
            jwk["use"] = json!("sig");
            jwk
        })
        .collect();
    json!({
        "client_id": id,
        "client_name": "The console",
        "client_uri": "https://console.test",
        "logo_uri": "https://console.test/logo.png",
        "redirect_uris": [REDIRECT],
        "scope": "atproto repo:app.bsky.feed.post",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "private_key_jwt",
        "token_endpoint_auth_signing_alg": "ES256",
        "application_type": "web",
        "dpop_bound_access_tokens": true,
        "jwks": {"keys": keys},
    })
}

/// A client assertion for `id`, signed by `key` as `kid`, as `change`
/// changes its claims.
fn assertion(
    h: &Running,
    id: &str,
    kid: &str,
    key: &Keypair,
    change: impl Fn(&mut Json),
) -> String {
    let mut claims = json!({
        "iss": id,
        "sub": id,
        "aud": h.host.config.public_url,
        "jti": jose::random(12),
        "iat": now(),
        "exp": now() + 60,
    });
    change(&mut claims);
    jose::sign(key, &json!({"kid": kid, "typ": "JWT"}), &claims)
}

fn asserting(id: &str, kid: &'static str, key: Arc<Keypair>) -> App {
    let mut app = App::new(id);
    let id = id.to_string();
    app.assert = Some(Box::new(move |h: &Running| {
        vec![
            (
                "client_assertion_type".into(),
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer".into(),
            ),
            (
                "client_assertion".into(),
                assertion(h, &id, kid, &key, |_| {}),
            ),
        ]
    }));
    app
}

async fn group(h: &Running) -> (String, Authority) {
    let [alice, ..] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    (did, alice)
}

#[tokio::test]
async fn a_confidential_client_signs_each_step_and_refreshes() {
    let h = start_reachable(&[]).await;
    let (docs, url) = documents().await;
    let id = format!("{url}/console.json");
    let key = Arc::new(Keypair::generate(Curve::P256));
    let doc = confidential(&id, &[("k1", &key)]);
    docs.lock()
        .unwrap()
        .insert("console.json".into(), (200, doc.to_string()));
    let (did, alice) = group(&h).await;
    let app = asserting(&id, "k1", key.clone());
    let tokens = app.sign_in(&h, &alice, &did).await;
    let said = app
        .refresh(&h, tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Its keys may be at a URL of their own.
    let mut by_uri = confidential(&format!("{url}/by-uri.json"), &[]);
    let jwks = by_uri["jwks"].take();
    let mut keys = jwks.clone();
    let mut jwk = jose::jwk(&key.public());
    jwk["kid"] = json!("k1");
    keys["keys"] = json!([jwk]);
    by_uri.as_object_mut().unwrap().remove("jwks");
    by_uri["jwks_uri"] = json!(format!("{url}/jwks.json"));
    docs.lock()
        .unwrap()
        .insert("by-uri.json".into(), (200, by_uri.to_string()));
    docs.lock()
        .unwrap()
        .insert("jwks.json".into(), (200, keys.to_string()));
    let app = asserting(&format!("{url}/by-uri.json"), "k1", key);
    app.push(&h, &[]).await;
}

/// A confidential client at `name` whose keys are at `keys`, holding
/// these.
fn keys_at(docs: &Docs, url: &str, name: &str, keys: &str, held: &[(&str, &Keypair)]) {
    let mut doc = confidential(&format!("{url}/{name}"), &[]);
    doc.as_object_mut().unwrap().remove("jwks");
    doc["jwks_uri"] = json!(format!("{url}/{keys}"));
    let held: Vec<Json> = held
        .iter()
        .map(|(kid, k)| {
            let mut jwk = jose::jwk(&k.public());
            jwk["kid"] = json!(kid);
            jwk
        })
        .collect();
    let mut docs = docs.lock().unwrap();
    docs.insert(name.into(), (200, doc.to_string()));
    docs.insert(keys.into(), (200, json!({"keys": held}).to_string()));
}

#[tokio::test]
async fn keeps_a_clients_keys_and_fetches_them_again_for_one_it_does_not_know() {
    let h = start_reachable(&[]).await;
    let (docs, url) = documents().await;
    let id = format!("{url}/by-uri.json");
    let [k1, k2, k3] = [(); 3].map(|_| Arc::new(Keypair::generate(Curve::P256)));
    keys_at(&docs, &url, "by-uri.json", "jwks.json", &[("k1", &k1)]);
    asserting(&id, "k1", k1.clone()).push(&h, &[]).await;
    // Kept: its key host may be down a while.
    docs.lock()
        .unwrap()
        .insert("jwks.json".into(), (500, String::new()));
    asserting(&id, "k1", k1.clone()).push(&h, &[]).await;
    // A key it does not know has it fetch them again, once.
    keys_at(&docs, &url, "by-uri.json", "jwks.json", &[("k2", &k2)]);
    asserting(&id, "k2", k2.clone()).push(&h, &[]).await;
    let said = asserting(&id, "k3", k3.clone()).push_said(&h).await;
    assert_eq!(said.error(), (400, "invalid_client"), "{}", said.body);
    // As when they were just fetched, without another fetch.
    let other = format!("{url}/other.json");
    keys_at(&docs, &url, "other.json", "other-jwks.json", &[("k1", &k1)]);
    let said = asserting(&other, "k3", k3).push_said(&h).await;
    assert_eq!(said.error(), (400, "invalid_client"), "{}", said.body);
}

#[tokio::test]
async fn refuses_an_assertion_that_is_not_the_clients_for_this_host() {
    let h = start_reachable(&[]).await;
    let (docs, url) = documents().await;
    let id = format!("{url}/console.json");
    let key = Keypair::generate(Curve::P256);
    let other = Keypair::generate(Curve::P256);
    docs.lock().unwrap().insert(
        "console.json".into(),
        (200, confidential(&id, &[("k1", &key)]).to_string()),
    );
    let app = App::new(&id);
    let challenge = jose::sha256(&app.verifier);
    let typ = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
    let sign = |change: &dyn Fn(&mut Json)| assertion(&h, &id, "k1", &key, change);
    let reused = sign(&|_| {});
    let first = app
        .post(
            &h,
            "/oauth/par",
            &app.request(
                &challenge,
                &[
                    ("client_assertion_type", typ),
                    ("client_assertion", &reused),
                ],
            ),
        )
        .await;
    assert_eq!(first.status, 201, "{}", first.body);
    let token_aud = sign(&|c| {
        c["aud"] = json!([
            "https://elsewhere.test",
            format!("{}/oauth/token", h.host.config.public_url)
        ])
    });
    let said = app
        .post(
            &h,
            "/oauth/par",
            &app.request(
                &challenge,
                &[
                    ("client_assertion_type", typ),
                    ("client_assertion", &token_aud),
                ],
            ),
        )
        .await;
    assert_eq!(said.status, 201, "{}", said.body);
    for (sent, kind, refused) in [
        (reused, typ, "used before"),
        ("not.a.jwt".to_string(), typ, "bad JSON"),
        (sign(&|_| {}), "\0", "authenticates with private_key_jwt"),
        (
            sign(&|_| {}),
            "urn:other",
            "authenticates with private_key_jwt",
        ),
        (
            assertion(&h, &id, "k1", &other, |_| {}),
            typ,
            "no key of the client",
        ),
        (
            assertion(&h, &id, "k2", &key, |_| {}),
            typ,
            "no key of the client",
        ),
        (
            sign(&|c| c["iss"] = json!("https://elsewhere.test")),
            typ,
            "the client's own",
        ),
        (
            sign(&|c| c["sub"] = json!("https://elsewhere.test")),
            typ,
            "the client's own",
        ),
        (
            sign(&|c| c["aud"] = json!("https://elsewhere.test")),
            typ,
            "the client's own",
        ),
        (
            sign(&|c| c["aud"] = json!(["https://elsewhere.test"])),
            typ,
            "the client's own",
        ),
        (sign(&|c| c["exp"] = json!(now() - 1)), typ, "too old"),
        (sign(&|c| c["iat"] = json!(now() - 600)), typ, "too old"),
        (
            sign(&|c| c["iat"] = json!(now() + 600)),
            typ,
            "from the future",
        ),
        (sign(&|c| c["jti"] = json!("")), typ, "without a jti"),
    ] {
        let form = app.request(
            &challenge,
            &[("client_assertion_type", kind), ("client_assertion", &sent)],
        );
        let said = app.post(&h, "/oauth/par", &form).await;
        assert_eq!(said.error(), (400, "invalid_client"), "{refused}");
        let why = said.body["error_description"].as_str().unwrap();
        assert!(why.contains(refused), "{refused}: {why}");
    }
    let said = app
        .post(&h, "/oauth/par", &app.request(&challenge, &[]))
        .await;
    assert_eq!(said.error(), (400, "invalid_client"));
}

#[tokio::test]
async fn holds_a_client_to_the_key_it_first_signed_with() {
    let h = start_reachable(&[]).await;
    let (docs, url) = documents().await;
    let id = format!("{url}/console.json");
    let (one, two) = (
        Arc::new(Keypair::generate(Curve::P256)),
        Arc::new(Keypair::generate(Curve::P256)),
    );
    let first_key_bytes = one.to_bytes();
    let doc = confidential(&id, &[("k1", &one), ("k2", &two)]);
    docs.lock()
        .unwrap()
        .insert("console.json".into(), (200, doc.to_string()));
    let (did, alice) = group(&h).await;
    let first = asserting(&id, "k1", one);
    let request_uri = first.push(&h, &[]).await;
    let (_, approved) = approve(&h, &alice, &request_uri, &did).await;
    let mut second = asserting(&id, "k2", two.clone());
    second.key = first.key.clone();
    second.verifier = first.verifier.clone();
    let said = second
        .exchange(&h, approved["redirectUrl"].as_str().unwrap())
        .await;
    assert_eq!(
        said.body["error_description"],
        "the client authenticates otherwise than it did"
    );
    let tokens = first.sign_in(&h, &alice, &did).await;
    let said = second
        .refresh(&h, tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(
        said.body["error_description"],
        "the client authenticates otherwise than it did"
    );
    // Naming no key is no way around it: the key that signed is the one held.
    let mut unnamed = asserting(&id, "", two.clone());
    unnamed.key = first.key.clone();
    let said = unnamed
        .refresh(&h, tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(
        said.body["error_description"],
        "the client authenticates otherwise than it did"
    );
    let mut same = asserting(
        &id,
        "",
        Arc::new(Keypair::from_bytes(Curve::P256, &first_key_bytes).unwrap()),
    );
    same.key = first.key.clone();
    let said = same
        .refresh(&h, tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
}

#[tokio::test]
async fn a_public_client_sends_no_assertion() {
    let h = start(&[], false).await;
    let app = App::loopback("atproto");
    let challenge = jose::sha256(&app.verifier);
    let form = app.request(&challenge, &[("client_assertion", "x")]);
    let said = app.post(&h, "/oauth/par", &form).await;
    assert_eq!(said.error(), (400, "invalid_client"));
}

#[tokio::test]
async fn checks_a_clients_metadata() {
    let h = start_reachable(&[]).await;
    let (docs, url) = documents().await;
    let key = Keypair::generate(Curve::P256);
    let now = group_host::time::now_millis();
    let cases: Vec<(&str, Change, &str)> = vec![
        (
            "id",
            Box::new(|m| m["client_id"] = json!("https://elsewhere.test")),
            "not the id",
        ),
        (
            "both",
            Box::new(|m| m["jwks_uri"] = json!("https://keys.test")),
            "not both",
        ),
        (
            "scope",
            Box::new(|m| m["scope"] = json!("repo:x")),
            "no atproto",
        ),
        (
            "grant",
            Box::new(|m| m["grant_types"] = json!(["authorization_code", "implicit"])),
            "implicit",
        ),
        (
            "code",
            Box::new(|m| m["response_types"] = json!(["token"])),
            "takes a code",
        ),
        (
            "code-grant",
            Box::new(|m| m["grant_types"] = json!(["refresh_token"])),
            "takes a code",
        ),
        (
            "dpop",
            Box::new(|m| m["dpop_bound_access_tokens"] = json!(false)),
            "dpop_bound",
        ),
        (
            "redirects",
            Box::new(|m| m["redirect_uris"] = json!([])),
            "redirect_uri",
        ),
        (
            "none-alg",
            Box::new(|m| m["token_endpoint_auth_method"] = json!("none")),
            "public client",
        ),
        (
            "no-keys",
            Box::new(|m| {
                m.as_object_mut().unwrap().remove("jwks");
            }),
            "needs jwks",
        ),
        (
            "no-alg",
            Box::new(|m| {
                m.as_object_mut()
                    .unwrap()
                    .remove("token_endpoint_auth_signing_alg");
            }),
            "signing_alg",
        ),
        (
            "native",
            Box::new(|m| m["application_type"] = json!("native")),
            "native",
        ),
        (
            "secret",
            Box::new(|m| m["token_endpoint_auth_method"] = json!("client_secret_basic")),
            "client_secret_basic",
        ),
    ];
    for (name, change, refused) in cases {
        let id = format!("{url}/{name}");
        let mut doc = confidential(&id, &[("k1", &key)]);
        change(&mut doc);
        docs.lock()
            .unwrap()
            .insert(name.into(), (200, doc.to_string()));
        let e = h.host.client(&id, now).await.unwrap_err();
        assert_eq!(e.error, "invalid_client_metadata");
        assert!(e.description.contains(refused), "{name}: {}", e.description);
    }
    // A public client's metadata, without a signing alg.
    let id = format!("{url}/public");
    let mut doc = confidential(&id, &[]);
    doc["token_endpoint_auth_method"] = json!("none");
    for k in ["jwks", "token_endpoint_auth_signing_alg"] {
        doc.as_object_mut().unwrap().remove(k);
    }
    docs.lock()
        .unwrap()
        .insert("public".into(), (200, doc.to_string()));
    assert_eq!(h.host.client(&id, now).await.unwrap().metadata, doc);
}

#[tokio::test]
async fn reads_a_document_only_where_it_should() {
    let h = start_reachable(&[]).await;
    let (docs, url) = documents().await;
    let now = group_host::time::now_millis();
    docs.lock().unwrap().insert(
        "large".into(),
        (200, format!("{{\"x\": \"{}\"}}", "a".repeat(70_000))),
    );
    docs.lock()
        .unwrap()
        .insert("text".into(), (200, "[1, 2]".into()));
    docs.lock()
        .unwrap()
        .insert("gone".into(), (500, "{}".into()));
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/x", listener.local_addr().unwrap())
    };
    for (id, refused) in [
        (format!("{url}/large"), "too large"),
        (format!("{url}/text"), "not a JSON object"),
        (format!("{url}/gone"), "answered 500"),
        (format!("{url}/nothing"), "answered 404"),
        (format!("{url}/x#fragment"), "not an https URL"),
        (format!("ftp://{}/x", &url[7..]), "not an https URL"),
        (closed, "client metadata at"),
        ("nonsense".into(), "not a URL"),
    ] {
        let e = h.host.client(&id, now).await.unwrap_err();
        assert!(e.description.contains(refused), "{id}: {}", e.description);
    }
    // Nor, for a host on the internet, anywhere but the internet.
    let https = start(&[], false).await;
    for id in [
        "https://127.0.0.1/client.json",
        "https://localhost/client.json",
        "https://10.0.0.1/client.json",
        "https://[::1]/client.json",
        "https://169.254.169.254/latest/meta-data",
    ] {
        let e = https.host.client(id, now).await.unwrap_err();
        assert!(
            e.description.contains("not at a public address"),
            "{id}: {}",
            e.description
        );
    }
    let e = https
        .host
        .client("https://nothing.invalid/client.json", now)
        .await
        .unwrap_err();
    assert!(
        e.description.contains("client metadata at"),
        "{}",
        e.description
    );
    // An http client is for a host on http alone.
    let e = https
        .host
        .client(&format!("{url}/text"), now)
        .await
        .unwrap_err();
    assert!(
        e.description.contains("not an https URL"),
        "{}",
        e.description
    );
}

#[tokio::test]
async fn takes_the_loopback_client_as_atproto_has_it() {
    let h = start(&[], false).await;
    let now = group_host::time::now_millis();
    let bare = h.host.client("http://localhost", now).await.unwrap();
    assert_eq!(
        bare.metadata["redirect_uris"],
        json!(["http://127.0.0.1/", "http://[::1]/"])
    );
    assert_eq!(bare.metadata["scope"], "atproto");
    assert_eq!(bare.metadata["token_endpoint_auth_method"], "none");
    let named = h
        .host
        .client(&loopback_id("atproto repo:x"), now)
        .await
        .unwrap();
    assert_eq!(named.metadata["redirect_uris"], json!([REDIRECT]));
    assert_eq!(named.metadata["scope"], "atproto repo:x");
    for (id, refused) in [
        ("http://localhost?client_name=x", "names client_name"),
        ("http://localhost/?scope=atproto&scope=atproto", "one scope"),
        (
            "http://localhost?redirect_uri=https%3A%2F%2Fapp.test%2F",
            "127.0.0.1",
        ),
        (
            "http://localhost?redirect_uri=http%3A%2F%2F127.0.0.1.evil.example%2Fcb",
            "127.0.0.1",
        ),
        (
            "http://localhost?redirect_uri=https%3A%2F%2F127.0.0.1%2Fcb",
            "127.0.0.1",
        ),
        ("http://localhost?redirect_uri=nonsense", "127.0.0.1"),
    ] {
        let e = h.host.client(id, now).await.unwrap_err();
        assert!(e.description.contains(refused), "{id}: {}", e.description);
    }
    // Another host's localhost is no loopback client.
    let e = h
        .host
        .client("http://localhost:8080", now)
        .await
        .unwrap_err();
    assert!(
        e.description.contains("not an https URL"),
        "{}",
        e.description
    );
}

#[test]
fn knows_a_public_address() {
    use group_host::oauth_clients::public;
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
        ("fd00::1", false),
        ("fe80::1", false),
        ("ff02::1", false),
        ("2001:db8::1", false),
    ] {
        assert_eq!(public(ip.parse().unwrap()), is, "{ip}");
    }
}

#[tokio::test]
async fn keeps_what_it_read_a_while() {
    let h = start_reachable(&[]).await;
    let (docs, url) = documents().await;
    let id = format!("{url}/console.json");
    let key = Keypair::generate(Curve::P256);
    let mut doc = confidential(&id, &[("k1", &key)]);
    docs.lock()
        .unwrap()
        .insert("console.json".into(), (200, doc.to_string()));
    let now = group_host::time::now_millis();
    h.host.client(&id, now).await.unwrap();
    doc["client_name"] = json!("Renamed");
    docs.lock()
        .unwrap()
        .insert("console.json".into(), (200, doc.to_string()));
    let kept = h.host.client(&id, now + 60_000).await.unwrap();
    assert_eq!(kept.metadata["client_name"], "The console");
    let read = h.host.client(&id, now + 11 * 60_000).await.unwrap();
    assert_eq!(read.metadata["client_name"], "Renamed");
    // A thousand others, and it starts over.
    for i in 0..1000 {
        h.host
            .client(&format!("http://localhost?scope=atproto%20x{i}"), now)
            .await
            .unwrap();
    }
    doc["client_name"] = json!("Renamed again");
    docs.lock()
        .unwrap()
        .insert("console.json".into(), (200, doc.to_string()));
    let read = h.host.client(&id, now + 12 * 60_000).await.unwrap();
    assert_eq!(read.metadata["client_name"], "Renamed again");
}
