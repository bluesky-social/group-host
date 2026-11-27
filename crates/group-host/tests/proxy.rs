//! An app signed in as a group calling other services through the host,
//! as `atproto-proxy` asks, and asking it for service auth: each call
//! signed as the group, as far as the token's scope and its member's
//! roles reach.

mod common;

use atproto::did;
use atproto::service_auth;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Json, Router};
use common::groups::*;
use common::oauth::{App, Said, approve};
use common::spaces::*;
use common::{Running, serve, start, start_reachable};
use group_host::group::members_space;
use group_host::permission_sets::included;
use group_host::repo::Write;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const SERVICE: &str = "did:web:grain.test";
const APPVIEW: &str = "did:web:grain.test#grain_appview";
const SET: &str = "test.grain.authFull";
const INCLUDE: &str = "include:test.grain.authFull?aud=did:web:grain.test%23grain_appview";

/// A call the service took: its method, NSID, query, the headers it
/// was sent and its body, and the service auth it carried, checked.
#[derive(Clone, Debug)]
struct Took {
    method: String,
    nsid: String,
    query: HashMap<String, String>,
    headers: HeaderMap,
    body: Vec<u8>,
    claims: service_auth::Claims,
}

/// Grain's AppView, faked: its DID document, the DNS of its lexicons, the
/// repo they are published in, and its methods, each checking the service
/// auth it is sent against the document of whoever signed it.
#[derive(Default)]
struct Service {
    url: Mutex<String>,
    plc: Mutex<String>,
    took: Mutex<Vec<Took>>,
}

fn set() -> Value {
    json!({
        "type": "permission-set",
        "title": "Grain test",
        "detail": "Everything Grain does",
        "permissions": [
            {"type": "permission", "resource": "repo", "collection": ["test.grain.photo", "test.grain.gallery"]},
            {"type": "permission", "resource": "repo", "collection": ["test.grain.like"], "action": ["create", "delete"]},
            {"type": "permission", "resource": "repo", "collection": ["app.bsky.feed.post"]},
            {"type": "permission", "resource": "rpc", "inheritAud": true, "lxm": ["test.grain.actor.getProfile", "test.grain.feed.post"]},
            {"type": "permission", "resource": "rpc", "aud": "*", "lxm": ["test.grain.describe"]},
            {"type": "permission", "resource": "rpc", "aud": "did:web:other.test#x", "lxm": ["test.grain.other"]},
            {"type": "permission", "resource": "rpc", "lxm": []},
            {"type": "permission", "resource": "space", "spaceType": "test.grain.space"},
        ],
    })
}

async fn did_json(State(s): State<Arc<Service>>) -> Json<Value> {
    let url = s.url.lock().unwrap().clone();
    let port = url.rsplit(':').next().unwrap().to_string();
    let service =
        |id: &str, at: String| json!({"id": format!("#{id}"), "type": "x", "serviceEndpoint": at});
    Json(json!({
        "id": SERVICE,
        "service": [
            service("grain_appview", url.clone()),
            service("atproto_pds", format!("{url}/")),
            service("private", format!("https://127.0.0.1:{port}")),
            service("closed", "http://127.0.0.1:1".into()),
        ],
    }))
}

/// TXT records as DNS-over-HTTPS's JSON gives them.
async fn dns(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let txt = |data: &str| json!({"name": q["name"], "type": 16, "data": data});
    let answer = match q["name"].as_str() {
        "_lexicon.grain.test" => vec![
            json!({"name": "_lexicon.grain.test", "type": 5, "data": "elsewhere."}),
            txt("\"did=did:web:\" \"grain.test\""),
            txt("\"v=spf1\""),
        ],
        "_lexicon.twice.test" => vec![
            txt("\"did=did:web:grain.test\""),
            txt("\"did=did:web:other.test\""),
        ],
        _ => return Json(json!({"Status": 3})),
    };
    assert_eq!(q["type"], "TXT");
    Json(json!({"Status": 0, "Answer": answer}))
}

async fn get_record(Query(q): Query<HashMap<String, String>>) -> Response {
    assert_eq!(
        (q["repo"].as_str(), q["collection"].as_str()),
        (SERVICE, "com.atproto.lexicon.schema")
    );
    let record = json!({"type": "record", "key": "tid"});
    let mut id = q["rkey"].clone();
    let mut description = None;
    let main = match q["rkey"].as_str() {
        SET => set(),
        "test.grain.notASet" => json!({"type": "query"}),
        "test.grain.photo" => json!({"type": "record", "description": "A photo in a gallery"}),
        "test.grain.gallery" => {
            description = Some("Galleries of photos");
            record
        }
        // Published under another's name.
        "test.grain.comment" => {
            id = "test.grain.photo".into();
            json!({"type": "record", "description": "A photo in a gallery"})
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "RecordNotFound"})),
            )
                .into_response();
        }
    };
    let mut value = json!({"$type": "com.atproto.lexicon.schema", "lexicon": 1, "id": id, "defs": {"main": main}});
    if let Some(description) = description {
        value["description"] = json!(description);
    }
    Json(json!({"uri": "at://x", "value": value})).into_response()
}

/// A method of the service's: its service auth checked first.
async fn method(
    State(s): State<Arc<Service>>,
    Path(nsid): Path<String>,
    m: Method,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let token = headers["authorization"]
        .to_str()
        .unwrap()
        .strip_prefix("Bearer ")
        .unwrap();
    let (_, unchecked) = service_auth::parts(token).unwrap();
    let plc = s.plc.lock().unwrap().clone();
    let doc: Value = reqwest::get(format!("{plc}/{}", unchecked.iss))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let key = did::signing_key(&doc).unwrap();
    let claims = service_auth::verify(token, &key, now()).unwrap();
    assert!(
        [SERVICE, APPVIEW].contains(&claims.aud.as_str()),
        "{}",
        claims.aud
    );
    assert_eq!(claims.lxm.as_deref(), Some(nsid.as_str()));
    s.took.lock().unwrap().push(Took {
        method: m.to_string(),
        nsid: nsid.clone(),
        query,
        headers,
        body: body.to_vec(),
        claims,
    });
    match nsid.as_str() {
        "test.grain.fail" => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("retry-after", "5"), ("content-language", "en")],
            Json(json!({"error": "Boom", "message": "it broke"})),
        )
            .into_response(),
        "test.grain.gone" => (StatusCode::NOT_FOUND, "nope").into_response(),
        "test.grain.teapot" => StatusCode::IM_A_TEAPOT.into_response(),
        _ => (
            [
                ("atproto-repo-rev", "rev1"),
                ("content-language", "en"),
                ("x-other", "no"),
            ],
            Json(json!({"echo": nsid})),
        )
            .into_response(),
    }
}

/// The service, at an address of its own.
async fn service() -> (Arc<Service>, String) {
    let s = Arc::new(Service::default());
    let router = Router::new()
        .route("/.well-known/did.json", routing::get(did_json))
        .route("/dns-query", routing::get(dns))
        .route("/xrpc/com.atproto.repo.getRecord", routing::get(get_record))
        .route("/xrpc/{nsid}", routing::get(method).post(method))
        .with_state(s.clone());
    let url = serve(router).await;
    *s.url.lock().unwrap() = url.clone();
    (s, url)
}

/// A host reaching the service as `grain.test`, and the service: at its
/// own address (`reachable`, on http) or as one on https would.
async fn network(reachable: bool) -> (Running, Arc<Service>) {
    let (s, url) = service().await;
    let address = url.strip_prefix("http://").unwrap();
    let hosts = format!("grain.test={address}");
    let dns = format!("{url}/dns-query");
    let vars = [
        ("DID_WEB_SCHEME", "http"),
        ("DID_WEB_HOSTS", hosts.as_str()),
        ("DNS_OVER_HTTPS", dns.as_str()),
    ];
    let h = match reachable {
        true => start_reachable(&vars).await,
        false => start(&vars, false).await,
    };
    *s.plc.lock().unwrap() = plc_url(&h);
    (h, s)
}

async fn put_membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let collection = format!("{NS}.membership");
    let value = json!({"$type": collection, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let space = members_space(did);
    let exists = h
        .host
        .group_record(did, &space, &collection, who)
        .unwrap()
        .is_some();
    let rkey = who.to_string();
    let write = match exists {
        true => Write::Update {
            collection,
            rkey,
            value,
        },
        false => Write::Create {
            collection,
            rkey,
            value,
        },
    };
    h.host.space_write(did, &space, &[write]).await.unwrap();
}

/// Riders, Alice's; Bob a publisher, who may do anything as the group;
/// Carol a photographer, who may post photos as it alone.
async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    for (rkey, permissions) in [
        ("publisher", json!(["repo:*"])),
        ("photographer", json!(["repo:test.grain.photo"])),
    ] {
        let value =
            json!({"$type": format!("{NS}.role"), "displayName": rkey, "permissions": permissions});
        let write = Write::Create {
            collection: format!("{NS}.role"),
            rkey: rkey.into(),
            value,
        };
        h.host
            .space_write(&did, &members_space(&did), &[write])
            .await
            .unwrap();
    }
    put_membership(h, &did, &bob.did, &["member", "publisher"]).await;
    put_membership(h, &did, &carol.did, &["member", "photographer"]).await;
    (did, [alice, bob, carol])
}

/// An app asking for `scope`, signed in as `group` by `member`: it, and
/// its access token.
async fn signed_in(h: &Running, member: &Authority, group: &str, scope: &str) -> (App, String) {
    let app = App::loopback(scope);
    let tokens = app.sign_in(h, member, group).await;
    let token = tokens["access_token"].as_str().unwrap().to_string();
    (app, token)
}

/// `nsid` (with its query) called with `app`'s `token` and `headers`,
/// `body` as JSON (a POST) or none (a GET), once more with the host's
/// nonce if it asks.
async fn call(
    h: &Running,
    app: &App,
    token: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> Said {
    let nsid = target.split('?').next().unwrap();
    let mut said = None;
    for _ in 0..2 {
        let url = format!("{}/xrpc/{target}", h.url);
        let (method, req) = match body {
            Some(b) => ("POST", h.http.post(url).json(b)),
            None => ("GET", h.http.get(url)),
        };
        let mut req = req
            .header("authorization", format!("DPoP {token}"))
            .header("dpop", app.resource_proof(h, method, nsid, token));
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let got = app.said(req).await;
        let stale = got
            .headers
            .get("www-authenticate")
            .is_some_and(|v| v.to_str().unwrap().contains("use_dpop_nonce"));
        said = Some(got);
        if !stale {
            break;
        }
    }
    said.unwrap()
}

/// `target` called with an app password's session.
async fn bearer(h: &Running, token: &str, target: &str, headers: &[(&str, &str)]) -> (u16, Value) {
    let mut req = h
        .http
        .get(format!("{}/xrpc/{target}", h.url))
        .bearer_auth(token);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = req.send().await.unwrap();
    (
        res.status().as_u16(),
        res.json().await.unwrap_or(Value::Null),
    )
}

fn to(service: &str) -> [(&'static str, &str); 1] {
    [("atproto-proxy", service)]
}

#[tokio::test]
async fn sends_a_call_on_signed_as_the_group_and_its_answer_back() {
    let (h, s) = network(true).await;
    let (did, [alice, ..]) = group(&h).await;
    let scope = format!("atproto {INCLUDE} blob:image/*");
    let (app, token) = signed_in(&h, &alice, &did, &scope).await;

    // A query, its parameters, and the headers an AppView reads.
    let headers = [
        ("atproto-proxy", APPVIEW),
        ("accept-language", "fr"),
        ("atproto-accept-labelers", "did:plc:labeler"),
        ("x-atproto-thing", "yes"),
        ("x-unrelated", "no"),
        ("content-type", "text/plain"),
    ];
    let said = call(
        &h,
        &app,
        &token,
        "test.grain.actor.getProfile?actor=alice&limit=2",
        &headers,
        None,
    )
    .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body, json!({"echo": "test.grain.actor.getProfile"}));
    assert_eq!(said.headers["atproto-repo-rev"], "rev1");
    assert_eq!(said.headers["content-language"], "en");
    assert!(said.headers.get("x-other").is_none());
    let took = s.took.lock().unwrap().pop().unwrap();
    assert_eq!(
        (took.method.as_str(), took.nsid.as_str()),
        ("GET", "test.grain.actor.getProfile")
    );
    assert_eq!(took.query["actor"], "alice");
    assert_eq!(took.query["limit"], "2");
    // Signed as the group, for the service's DID alone, and that method.
    assert_eq!(took.claims.iss, did);
    assert_eq!(took.claims.aud, SERVICE);
    assert!(took.claims.exp <= now() + 60);
    assert_eq!(took.headers["accept-language"], "fr");
    assert_eq!(took.headers["atproto-accept-labelers"], "did:plc:labeler");
    assert_eq!(took.headers["x-atproto-thing"], "yes");
    // The encodings the app takes, as it asked.
    assert_eq!(took.headers["accept-encoding"], "gzip");
    assert!(took.headers.get("x-unrelated").is_none());
    assert!(took.headers.get("content-type").is_none());
    assert!(took.headers.get("dpop").is_none());

    // A procedure, its body as sent.
    let body = json!({"text": "hello"});
    let said = call(
        &h,
        &app,
        &token,
        "test.grain.feed.post",
        &to(APPVIEW),
        Some(&body),
    )
    .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let took = s.took.lock().unwrap().pop().unwrap();
    assert_eq!(took.method, "POST");
    assert_eq!(serde_json::from_slice::<Value>(&took.body).unwrap(), body);
    assert_eq!(took.headers["content-type"], "application/json");

    // A method the set holds for any service.
    let said = call(&h, &app, &token, "test.grain.describe", &to(APPVIEW), None).await;
    assert_eq!(said.status, 200, "{}", said.body);

    // A failure comes back as the service named it, but a 500 as the
    // upstream's failure.
    for (nsid, status, error, message) in [
        ("test.grain.fail", 502, "Boom", "it broke"),
        (
            "test.grain.gone",
            404,
            "XRPCNotSupported",
            "XRPCNotSupported",
        ),
        ("test.grain.teapot", 418, "Unknown", "Unknown"),
    ] {
        let scope = format!("atproto rpc:{nsid}?aud=did:web:grain.test%23grain_appview");
        let (app, token) = signed_in(&h, &alice, &did, &scope).await;
        let said = call(&h, &app, &token, nsid, &to(APPVIEW), None).await;
        assert_eq!(said.error(), (status, error), "{nsid}: {}", said.body);
        assert_eq!(said.body["message"], message);
        if status == 502 {
            assert_eq!(said.headers["retry-after"], "5");
            assert!(said.headers.get("content-language").is_none());
        }
    }
}

#[tokio::test]
async fn reads_another_apps_permission_set_as_it_publishes_it() {
    let (h, _) = network(true).await;
    let (did, [alice, _, carol]) = group(&h).await;
    let set = set();
    assert_eq!(
        included(INCLUDE, &set),
        [
            "repo:test.grain.photo",
            "repo:test.grain.gallery",
            "repo:test.grain.like?action=create&action=delete",
            "rpc:test.grain.actor.getProfile?aud=did:web:grain.test%23grain_appview",
            "rpc:test.grain.feed.post?aud=did:web:grain.test%23grain_appview",
            "rpc:test.grain.describe?aud=*",
        ]
    );
    // With no audience to inherit, its methods for a service it names
    // alone.
    let without = included("include:test.grain.authFull", &set);
    assert!(
        without.iter().all(|s| !s.contains("getProfile")),
        "{without:?}"
    );
    assert!(without.contains(&"rpc:test.grain.describe?aud=*".to_string()));

    let asked: Vec<String> = [
        "atproto",
        INCLUDE,
        "include:test.nowhere.authFull",
        "include:test.twice.authFull",
        "include:test.grain.notASet",
        "include:test.grain.missing",
        "include:org.atgrp.authParticipate",
        "include:nodots",
        "blob:image/*",
    ]
    .map(String::from)
    .into();
    let sets = h.host.permission_sets(&asked).await;
    assert_eq!(sets.keys().collect::<Vec<_>>(), [INCLUDE]);
    // Kept, it is not read again.
    assert_eq!(h.host.permission_sets(&asked).await, sets);
    let roles = |r: &[&str]| r.iter().map(|r| r.to_string()).collect::<Vec<_>>();
    // Whoever may do anything as the group keeps it whole.
    let all = h
        .host
        .narrow(&did, &roles(&["admin"]), &asked, &sets)
        .unwrap();
    assert_eq!(all, ["atproto", INCLUDE, "blob:image/*"]);
    let concrete = group_host::oauth_scope::concrete(&all, &did, &sets);
    assert!(
        concrete.contains("rpc:test.grain.feed.post?aud=did:web:grain.test%23grain_appview"),
        "{concrete}"
    );
    // A photographer, what of it they may write.
    let some = h
        .host
        .narrow(&did, &roles(&["member", "photographer"]), &asked, &sets)
        .unwrap();
    assert_eq!(some, ["atproto", "repo:test.grain.photo", "blob:image/*"]);
    let none = h
        .host
        .narrow(&did, &roles(&["member"]), &asked, &sets)
        .unwrap();
    assert_eq!(none, ["atproto"]);

    // The approval page names it in its own words.
    let app = App::loopback(&asked.join(" "));
    let request_uri = app.push(&h, &[]).await;
    let lxm = format!("{INTERNAL}.getAuthorization");
    let (status, view) = get(
        &h,
        &lxm,
        &format!("requestUri={request_uri}"),
        &[auth(&h, &carol, &lxm)],
    )
    .await;
    assert_eq!(status, 200, "{view}");
    let labels = view["labels"].as_array().unwrap();
    assert!(
        labels.contains(
            &json!({"scope": INCLUDE, "title": "Grain test", "text": "Everything Grain does"})
        ),
        "{view}"
    );
    let (status, _) = approve(&h, &alice, &request_uri, &did).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn says_what_another_apps_records_are_as_their_lexicons_describe_them() {
    let (h, _) = network(true).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let kinds = [
        "test.grain.photo",
        "test.grain.gallery",
        "test.grain.comment",
        "test.grain.like",
        "app.bsky.feed.post",
    ];
    let scope: Vec<String> = kinds.iter().map(|k| format!("repo:{k}")).collect();
    let (app, token) = signed_in(&h, &bob, &did, &format!("atproto {}", scope.join(" "))).await;
    for kind in kinds {
        let record = json!({"$type": kind, "createdAt": "2026-11-26T00:00:00Z"});
        let body = json!({"repo": did, "collection": kind, "rkey": "one", "record": record});
        let said = app
            .call(&h, "com.atproto.repo.createRecord", &token, Some(&body))
            .await;
        assert_eq!(said.status, 200, "{}", said.body);
    }
    let lxm = format!("{INTERNAL}.listGroupTokens");
    let query = format!("group={did}");
    let (status, listed) = get(&h, &lxm, &query, &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(status, 200, "{listed}");
    let described: HashMap<&str, Option<&str>> = listed["activity"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| {
            let write = &a["write"];
            let kind = write["collection"].as_str()?;
            Some((kind, write["description"].as_str()))
        })
        .collect();
    // Its main def's description, or else the lexicon's own; nothing for
    // one published under another's name, one not published, or one whose
    // publisher cannot be found.
    assert_eq!(
        described,
        HashMap::from([
            ("test.grain.photo", Some("A photo in a gallery")),
            ("test.grain.gallery", Some("Galleries of photos")),
            ("test.grain.comment", None),
            ("test.grain.like", None),
            ("app.bsky.feed.post", None),
        ])
    );
    // The app, as the loopback client is named.
    let session = &listed["sessions"][0];
    assert_eq!(
        session["client"],
        json!({"clientId": app.id, "name": "An app on this device"})
    );
}

#[tokio::test]
async fn refuses_a_call_the_scope_does_not_hold() {
    let (h, s) = network(true).await;
    let (did, [alice, ..]) = group(&h).await;
    let scope = "atproto rpc:test.grain.actor.getProfile?aud=did:web:grain.test%23grain_appview";
    let (app, token) = signed_in(&h, &alice, &did, scope).await;
    for (nsid, service) in [
        ("test.grain.feed.post", APPVIEW),
        (
            "test.grain.actor.getProfile",
            "did:web:grain.test#atproto_pds",
        ),
    ] {
        let said = call(&h, &app, &token, nsid, &to(service), None).await;
        assert_eq!(said.error(), (403, "ScopeMissingError"), "{}", said.body);
        let needed = format!("rpc:{nsid}?aud={}", service.replace('#', "%23"));
        assert_eq!(
            said.body["message"],
            format!("Missing required scope \"{needed}\"")
        );
    }
    assert!(s.took.lock().unwrap().is_empty());

    // `transition:generic` holds any method but chat's;
    // `transition:chat.bsky`, chat's.
    let (app, token) = signed_in(&h, &alice, &did, "atproto transition:generic").await;
    let said = call(&h, &app, &token, "test.grain.anything", &to(APPVIEW), None).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let said = call(
        &h,
        &app,
        &token,
        "chat.bsky.convo.listConvos",
        &to(APPVIEW),
        None,
    )
    .await;
    assert_eq!(said.error(), (403, "ScopeMissingError"));
    let (app, token) = signed_in(
        &h,
        &alice,
        &did,
        "atproto transition:generic transition:chat.bsky",
    )
    .await;
    let said = call(
        &h,
        &app,
        &token,
        "chat.bsky.convo.listConvos",
        &to(APPVIEW),
        None,
    )
    .await;
    assert_eq!(said.status, 200, "{}", said.body);

    // A token for its own spaces calls no other service.
    let (app, token) = signed_in(&h, &alice, &did, "atproto repo:test.grain.photo").await;
    let said = call(&h, &app, &token, "test.grain.anything", &to(APPVIEW), None).await;
    assert_eq!(said.error(), (403, "ScopeMissingError"));
}

#[tokio::test]
async fn stops_once_the_member_could_no_longer_give_it() {
    let (h, _) = network(true).await;
    let (did, [_, bob, _]) = group(&h).await;
    let scope = "atproto transition:generic transition:chat.bsky";
    // A photographer gives no other service's methods.
    let roles = vec!["member".to_string(), "photographer".to_string()];
    let asked: Vec<String> = scope.split(' ').map(String::from).collect();
    let given = h
        .host
        .narrow(&did, &roles, &asked, &Default::default())
        .unwrap();
    assert_eq!(given, ["atproto"]);
    let (app, token) = signed_in(&h, &bob, &did, scope).await;
    let said = call(&h, &app, &token, "test.grain.anything", &to(APPVIEW), None).await;
    assert_eq!(said.status, 200, "{}", said.body);
    put_membership(&h, &did, &bob.did, &["member"]).await;
    let said = call(&h, &app, &token, "test.grain.anything", &to(APPVIEW), None).await;
    assert_eq!(said.error(), (403, "Forbidden"), "{}", said.body);
}

#[tokio::test]
async fn an_app_passwords_session_calls_anything_but_chat_unless_it_may() {
    let (h, s) = network(true).await;
    let (did, [alice, ..]) = group(&h).await;
    for (privileged, chat) in [(false, 400), (true, 200)] {
        let lxm = format!("{INTERNAL}.createAppPassword");
        let body =
            json!({"group": did, "name": format!("p{privileged}"), "privileged": privileged});
        let (status, made) = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
        assert_eq!(status, 200, "{made}");
        let password = made["password"].as_str().unwrap();
        let signed = json!({"identifier": RIDERS, "password": password});
        let (status, session) = post(&h, "com.atproto.server.createSession", &signed, &[]).await;
        assert_eq!(status, 200, "{session}");
        let access = session["accessJwt"].as_str().unwrap();
        let (status, said) = bearer(&h, access, "test.grain.anything?x=1", &to(APPVIEW)).await;
        assert_eq!(status, 200, "{said}");
        assert_eq!(s.took.lock().unwrap().pop().unwrap().claims.iss, did);
        let (status, said) = bearer(&h, access, "chat.bsky.convo.listConvos", &to(APPVIEW)).await;
        assert_eq!(status, chat, "{said}");
        if chat == 400 {
            assert_eq!(said["error"], "InvalidToken");
            assert_eq!(said["message"], "Bad token method");
        }
    }
}

#[tokio::test]
async fn refuses_what_it_never_proxies() {
    let (h, _) = network(true).await;
    let (did, [alice, ..]) = group(&h).await;
    let (app, token) = signed_in(&h, &alice, &did, "atproto transition:generic").await;
    let service_did = h.host.config.service_did.clone();
    let own = format!("{service_did}#atgrp");
    let group_pds = format!("{did}#atproto_pds");
    for (nsid, header, status, error, message) in [
        // Not a method of the host's, nor one sent on: as before.
        (
            "test.grain.anything",
            None,
            501,
            "MethodNotImplemented",
            "test.grain.anything is not a method this host answers",
        ),
        (
            "com.atproto.server.updateEmail",
            Some(APPVIEW),
            400,
            "InvalidToken",
            "Bad token method",
        ),
        (
            "com.atproto.admin.getAccountInfo",
            Some(APPVIEW),
            400,
            "InvalidToken",
            "Bad token method",
        ),
        (
            "test.grain.x",
            Some("did:web:grain.test"),
            400,
            "InvalidRequest",
            "no service id specified in proxy header",
        ),
        (
            "test.grain.x",
            Some("did:web:grain.test#"),
            400,
            "InvalidRequest",
            "no service id specified in proxy header",
        ),
        (
            "test.grain.x",
            Some("#grain_appview"),
            400,
            "InvalidRequest",
            "no did specified in proxy header",
        ),
        (
            "test.grain.x",
            Some("did:web:grain.test#a#b"),
            400,
            "InvalidRequest",
            "invalid proxy header format",
        ),
        (
            "test.grain.x",
            Some("did:web:grain.test #a"),
            400,
            "InvalidRequest",
            "proxy header cannot contain spaces",
        ),
        (
            "test.grain.x",
            Some(&own),
            400,
            "InvalidRequest",
            "a call is never proxied to this host itself",
        ),
        (
            "test.grain.x",
            Some(&group_pds),
            400,
            "InvalidRequest",
            "a call is never proxied to this host itself",
        ),
        (
            "test.grain.x",
            Some("did:example:nobody#svc"),
            400,
            "InvalidRequest",
            "could not resolve proxy did",
        ),
        (
            "test.grain.x",
            Some("did:web:grain.test#unknown"),
            400,
            "InvalidRequest",
            "could not resolve proxy did service url",
        ),
        (
            "test.grain.x",
            Some("did:web:grain.test#closed"),
            502,
            "UpstreamFailure",
            "Upstream service unreachable",
        ),
    ] {
        let headers: Vec<(&str, &str)> = header.map(|h| ("atproto-proxy", h)).into_iter().collect();
        let said = call(&h, &app, &token, nsid, &headers, None).await;
        assert_eq!(
            said.error(),
            (status, error),
            "{nsid} {header:?}: {}",
            said.body
        );
        assert_eq!(said.body["message"], message, "{header:?}");
    }
    // A header that is no text.
    let mut req = h.http.get(format!("{}/xrpc/test.grain.x", h.url));
    req = req.header(
        "atproto-proxy",
        reqwest::header::HeaderValue::from_bytes(b"did:web:\xffx#y").unwrap(),
    );
    let said = app.said(req).await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
    // With no token, nothing is sent on.
    let (status, said) = get(&h, "test.grain.x", "", &[("atproto-proxy", APPVIEW.into())]).await;
    assert_eq!((status, said["error"].as_str()), (401, Some("AuthMissing")));
    // From a browser, anywhere.
    let res = h
        .http
        .request(
            reqwest::Method::OPTIONS,
            format!("{}/xrpc/test.grain.x", h.url),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    let allowed = res.headers()["access-control-allow-headers"]
        .to_str()
        .unwrap();
    assert!(allowed.contains("atproto-proxy"), "{allowed}");
}

#[tokio::test]
async fn reaches_a_service_only_at_a_public_address() {
    let (h, _) = network(false).await;
    let (did, [alice, ..]) = group(&h).await;
    let (app, token) = signed_in(&h, &alice, &did, "atproto transition:generic").await;
    for (service, why) in [
        ("did:web:grain.test#private", "is not at a public address"),
        ("did:web:grain.test#grain_appview", "is not on https"),
    ] {
        let said = call(&h, &app, &token, "test.grain.x", &to(service), None).await;
        assert_eq!(said.error(), (502, "UpstreamFailure"), "{}", said.body);
        assert!(
            said.body["message"].as_str().unwrap().contains(why),
            "{}",
            said.body
        );
    }
}

/// What `getServiceAuth` answered `app`'s `token` for `query`.
async fn service_auth(h: &Running, app: &App, token: &str, query: &str) -> Said {
    let target = format!("com.atproto.server.getServiceAuth?{query}");
    call(h, app, token, &target, &[], None).await
}

#[tokio::test]
async fn gives_service_auth_the_service_takes() {
    let (h, s) = network(true).await;
    let (did, [alice, ..]) = group(&h).await;
    let scope = format!("atproto {INCLUDE}");
    let (app, token) = signed_in(&h, &alice, &did, &scope).await;
    let aud = "did:web:grain.test%23grain_appview";
    let exp = now() + 600;
    let said = service_auth(
        &h,
        &app,
        &token,
        &format!("aud={aud}&lxm=test.grain.feed.post&exp={exp}"),
    )
    .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let jwt = said.body["token"].as_str().unwrap();
    let (_, claims) = service_auth::parts(jwt).unwrap();
    assert_eq!(
        (claims.iss.as_str(), claims.aud.as_str()),
        (did.as_str(), APPVIEW)
    );
    assert_eq!(claims.exp, exp);
    // The service takes it, checked against the group's document.
    let url = format!("{}/xrpc/test.grain.feed.post", s.url.lock().unwrap());
    let res = h.http.post(url).bearer_auth(jwt).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(s.took.lock().unwrap().pop().unwrap().claims.iss, did);

    for (query, status, error, message) in [
        (
            "aud=did:web:grain.test%23grain_appview&lxm=test.grain.other",
            403,
            "ScopeMissingError",
            "Missing required scope \"rpc:test.grain.other?aud=did:web:grain.test%23grain_appview\"",
        ),
        (
            "aud=did:web:grain.test%23grain_appview",
            403,
            "ScopeMissingError",
            "Missing required scope \"rpc:*?aud=did:web:grain.test%23grain_appview\"",
        ),
        (
            "lxm=test.grain.feed.post",
            400,
            "InvalidRequest",
            "aud is required",
        ),
    ] {
        let said = service_auth(&h, &app, &token, query).await;
        assert_eq!(said.error(), (status, error), "{query}: {}", said.body);
        assert_eq!(said.body["message"], message);
    }

    let (app, token) = signed_in(&h, &alice, &did, "atproto transition:generic").await;
    let now = now();
    let soon = now + 30;
    let ok = service_auth(&h, &app, &token, "aud=did:web:grain.test").await;
    assert_eq!(ok.status, 200, "{}", ok.body);
    let (_, claims) = service_auth::parts(ok.body["token"].as_str().unwrap()).unwrap();
    assert!(claims.lxm.is_none() && claims.exp <= now + 61, "{claims:?}");
    let ok = service_auth(
        &h,
        &app,
        &token,
        &format!("aud=did:web:grain.test&exp={soon}"),
    )
    .await;
    assert_eq!(ok.status, 200, "{}", ok.body);
    for (query, error, message) in [
        (
            "aud=grain.test".to_string(),
            "InvalidRequest",
            "aud must be a valid atproto DID or did#serviceId reference",
        ),
        (
            "aud=did:web:grain.test%23".into(),
            "InvalidRequest",
            "aud must be a valid atproto DID or did#serviceId reference",
        ),
        (
            "aud=did:web:grain.test%23a%23b".into(),
            "InvalidRequest",
            "aud must be a valid atproto DID or did#serviceId reference",
        ),
        (
            "aud=did:web:grain.test&exp=soon".into(),
            "InvalidRequest",
            "exp is a number of seconds",
        ),
        (
            format!("aud=did:web:grain.test&lxm=x.y.z&exp={}", now - 10),
            "BadExpiration",
            "expiration is in past",
        ),
        (
            format!("aud=did:web:grain.test&lxm=x.y.z&exp={}", now + 7200),
            "BadExpiration",
            "cannot request a token with an expiration more than an hour in the future",
        ),
        (
            format!("aud=did:web:grain.test&exp={}", now + 600),
            "BadExpiration",
            "cannot request a method-less token with an expiration more than a minute in the future",
        ),
        (
            "aud=did:web:grain.test&lxm=com.atproto.server.createAppPassword".into(),
            "InvalidRequest",
            "cannot request a service auth token for the following protected method: com.atproto.server.createAppPassword",
        ),
    ] {
        let said = service_auth(&h, &app, &token, &query).await;
        assert_eq!(said.error(), (400, error), "{query}: {}", said.body);
        assert_eq!(said.body["message"], message, "{query}");
    }

    // An app password's session, for anything but chat unless it may.
    let lxm = format!("{INTERNAL}.createAppPassword");
    let body = json!({"group": did, "name": "Phone"});
    let (_, made) = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
    let signed = json!({"identifier": RIDERS, "password": made["password"]});
    let (_, session) = post(&h, "com.atproto.server.createSession", &signed, &[]).await;
    let access = session["accessJwt"].as_str().unwrap();
    let target = "com.atproto.server.getServiceAuth?aud=did:web:grain.test&lxm=test.grain.x";
    let (status, said) = bearer(&h, access, target, &[]).await;
    assert_eq!(status, 200, "{said}");
    let target =
        "com.atproto.server.getServiceAuth?aud=did:web:grain.test&lxm=chat.bsky.convo.listConvos";
    let (status, said) = bearer(&h, access, target, &[]).await;
    assert_eq!(
        (status, said["error"].as_str()),
        (400, Some("InvalidRequest"))
    );
    assert_eq!(
        said["message"],
        "insufficient access to request a service auth token for the following method: chat.bsky.convo.listConvos"
    );
}
