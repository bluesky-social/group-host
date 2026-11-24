//! Moving an account here from the console: the host signs in to it at its
//! own server, for the person moving it, and does each step itself.
//! What matters: the account's server is asked only what a move needs; the
//! person moving it stays who they are; each call to it is bound to the
//! host's key, takes the nonce it is asked for, and outlives a token that
//! lapses; and the account, its spaces and its identity arrive as the
//! import methods bring them, after which the tokens are given back.

mod common;

use atproto::mst::Tree;
use atproto::plc::{Op, Service};
use atproto::service_auth::{self, Claims};
use atproto::space::{Commit as SpaceCommit, LtHash, element, star, write_star};
use atproto::{Cid, Commit, Curve, Keypair, car, drisl, jose};
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use common::groups::NS;
use common::spaces::plc_url;
use common::{Running, serve, start_reachable};
use serde_json::{Value as Json_, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

const PHOTO: &[u8] = &[9, 8, 7];
const CODE: &str = "plc-code-1";

/// Both the PDS the account and the person moving it live on and its
/// authorization server: it approves at once, for whichever DID a sign-in
/// names, and answers only calls bound to a token it gave and the key it
/// was given for.
#[derive(Default)]
struct Old {
    base: Mutex<String>,
    pushed: Mutex<Vec<HashMap<String, String>>>,
    /// The tokens it gave, and those it takes as lapsed, once each.
    tokens: Mutex<Vec<String>>,
    lapsed: Mutex<Vec<String>>,
    /// How long the first token it gives lasts, in seconds.
    first_lasts: Mutex<i64>,
    revoked: Mutex<Vec<String>>,
    /// The account: its DID, key and rotation key, its last operation,
    /// and its spaces.
    did: Mutex<String>,
    key: Mutex<Option<Keypair>>,
    rotation: Mutex<Option<Keypair>>,
    last: Mutex<Option<Cid>>,
    asked_code: Mutex<u32>,
    calls: Mutex<Vec<String>>,
    /// Whether it says it holds spaces, as the space alpha's servers do;
    /// whether it fails to say anything; and whether its tokens come with
    /// no refresh token.
    spaces: Mutex<bool>,
    undescribed: Mutex<bool>,
    no_refresh: Mutex<bool>,
    /// Whether it asks for a DPoP nonce without giving one.
    nonceless: Mutex<bool>,
}

impl Old {
    fn base(&self) -> String {
        self.base.lock().unwrap().clone()
    }
    fn did(&self) -> String {
        self.did.lock().unwrap().clone()
    }
    fn key(&self) -> Keypair {
        self.key.lock().unwrap().clone().unwrap()
    }
    fn forum(&self) -> String {
        format!("at://{}/space/com.example.forum/general", self.did())
    }
    fn empty(&self) -> String {
        format!("at://{}/space/com.example.diary/self", self.did())
    }
}

/// A nonce first, at the token endpoint, as atproto's servers ask.
fn nonce_first(headers: &HeaderMap) -> Option<Response> {
    let proof = headers["dpop"].to_str().unwrap();
    let (_, claims) = jose::parts(proof).unwrap();
    let ask = || {
        let body = Json(json!({"error": "use_dpop_nonce"}));
        (StatusCode::BAD_REQUEST, [("dpop-nonce", "n1")], body).into_response()
    };
    (claims["nonce"] != "n1").then(ask)
}

async fn resource(State(old): State<Arc<Old>>) -> Json<Json_> {
    Json(json!({ "authorization_servers": [old.base()] }))
}

async fn server(State(old): State<Arc<Old>>) -> Json<Json_> {
    let base = old.base();
    Json(json!({
        "issuer": base,
        "pushed_authorization_request_endpoint": format!("{base}/par"),
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "revocation_endpoint": format!("{base}/revoke"),
    }))
}

async fn par(
    State(old): State<Arc<Old>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Some(ask) = nonce_first(&headers) {
        return ask;
    }
    old.pushed.lock().unwrap().push(form);
    Json(json!({"request_uri": "urn:ietf:params:oauth:request_uri:r1"})).into_response()
}

async fn authorize(State(old): State<Arc<Old>>) -> Redirect {
    let pushed = old.pushed.lock().unwrap().last().cloned().unwrap();
    let mut to = reqwest::Url::parse(&pushed["redirect_uri"]).unwrap();
    to.query_pairs_mut()
        .append_pair("code", &pushed["login_hint"])
        .append_pair("state", &pushed["state"])
        .append_pair("iss", &old.base());
    Redirect::to(to.as_str())
}

async fn token(
    State(old): State<Arc<Old>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Some(ask) = nonce_first(&headers) {
        return ask;
    }
    let mut given = old.tokens.lock().unwrap();
    let access = format!("at{}", given.len() + 1);
    given.push(access.clone());
    // The code is the DID the sign-in named; a refresh is the account's.
    let sub = match form["grant_type"].as_str() {
        "refresh_token" => old.did(),
        _ => form["code"].clone(),
    };
    let lasts = std::mem::replace(&mut *old.first_lasts.lock().unwrap(), 3600);
    let mut tokens = json!({"access_token": access, "refresh_token": format!("r{access}"), "token_type": "DPoP", "sub": sub, "expires_in": lasts});
    if *old.no_refresh.lock().unwrap() {
        tokens.as_object_mut().unwrap().remove("refresh_token");
    }
    Json(tokens).into_response()
}

async fn revoke(
    State(old): State<Arc<Old>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Some(ask) = nonce_first(&headers) {
        return ask;
    }
    old.revoked.lock().unwrap().push(form["token"].clone());
    StatusCode::OK.into_response()
}

/// Whether a call is bound to a token this server gave, by the key it was
/// given for: a refusal if not, asking for a nonce first, as atproto's
/// servers do, and taking a lapsed token as lapsed once.
fn bound(old: &Old, headers: &HeaderMap, nsid: &str) -> Option<Response> {
    let nonceless = *old.nonceless.lock().unwrap();
    let refuse = |error: &str| {
        let challenge = format!("DPoP error=\"{error}\"");
        let headers = [("www-authenticate", challenge), ("dpop-nonce", "n2".into())];
        let mut res = (
            StatusCode::UNAUTHORIZED,
            headers,
            Json(json!({"error": error})),
        )
            .into_response();
        if nonceless {
            res.headers_mut().remove("dpop-nonce");
        }
        Some(res)
    };
    let auth = headers["authorization"].to_str().unwrap();
    let access = auth.strip_prefix("DPoP ").unwrap();
    let (_, claims) = jose::parts(headers["dpop"].to_str().unwrap()).unwrap();
    assert_eq!(claims["ath"], jose::sha256(access));
    assert!(
        claims["htu"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/xrpc/{nsid}"))
    );
    if claims["nonce"] != "n2" {
        return refuse("use_dpop_nonce");
    }
    let mut lapsed = old.lapsed.lock().unwrap();
    if let Some(at) = lapsed.iter().position(|t| t == access) {
        lapsed.remove(at);
        return refuse("invalid_token");
    }
    let given = old.tokens.lock().unwrap().contains(&access.to_string());
    let revoked = old.revoked.lock().unwrap().contains(&access.to_string());
    old.calls.lock().unwrap().push(nsid.into());
    (!given || revoked).then(|| StatusCode::FORBIDDEN.into_response())
}

/// A record's DRISL, from its JSON.
fn encoded(value: Json_) -> Vec<u8> {
    drisl::encode(&drisl::Value::from_json(&value).unwrap())
}

/// `did`'s public repo, signed with `key`, of one post.
fn public_repo(did: &str, key: &Keypair) -> Vec<u8> {
    let post = encoded(
        json!({"$type": "app.bsky.feed.post", "text": "hello", "createdAt": "2026-11-01T00:00:00Z"}),
    );
    let paths = BTreeMap::from([(
        "app.bsky.feed.post/3lzpost".to_string(),
        Cid::of_cbor(&post),
    )]);
    let tree = Tree::build(&paths);
    let commit = Commit::sign(did, tree.root(), "3l5mz5mkbn22b", key);
    let mut blocks: Vec<(Cid, Vec<u8>)> = tree.blocks().into_iter().collect();
    blocks.push((Cid::of_cbor(&post), post));
    blocks.push((commit.cid(), commit.to_bytes()));
    car::write(
        &[commit.cid()],
        blocks.iter().map(|(c, b)| (c, b.as_slice())),
    )
}

/// The forum's meta archive: a memberspace anyone reads.
fn forum_meta(space: &str) -> Vec<u8> {
    let value = |j: Json_| encoded(j);
    let meta =
        drisl::Value::from_json(&json!({"$type": "atproto-space-meta", "ver": 1, "space": space}));
    let drisl::Value::Map(meta) = meta.unwrap() else {
        panic!("a map")
    };
    let policy = value(json!({"$type": "com.atproto.memberspace.policy", "publicRead": true}));
    write_star(&meta, [(&b"policy"[..], &policy[..])])
}

/// `did`'s repo in `space`, of a note holding the photo.
fn space_repo(did: &str, space: &str) -> Vec<u8> {
    let photo = Cid::of_raw(PHOTO).to_string();
    let note = encoded(
        json!({"$type": "com.example.note", "text": "the harbor", "image": {"$type": "blob", "ref": {"$link": photo}, "mimeType": "image/png", "size": 3}, "createdAt": "2026-11-01T00:00:00Z"}),
    );
    let records = BTreeMap::from([("com.example.note/1".to_string(), note)]);
    let mut hash = LtHash::default();
    for (path, value) in &records {
        let (collection, rkey) = path.split_once('/').unwrap();
        hash.add(&element(collection, rkey, &Cid::of_cbor(value)));
    }
    let commit = SpaceCommit {
        repo: did.into(),
        space: space.into(),
        rev: "3lzaaaaaaaaad".into(),
        data: hash.digest(),
    };
    star(&commit, &records)
}

/// The account's server's own methods, each bound to its tokens.
async fn xrpc(
    State(old): State<Arc<Old>>,
    axum::extract::Path(nsid): axum::extract::Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // What anyone may read: the account's public repo, its files, and who
    // its handle names.
    match nsid.as_str() {
        "com.atproto.sync.getRepo" => return public_repo(&old.did(), &old.key()).into_response(),
        "com.atproto.sync.listBlobs" => return Json(json!({"cids": []})).into_response(),
        "com.atproto.server.describeServer" => {
            if *old.undescribed.lock().unwrap() {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            let spaces = *old.spaces.lock().unwrap();
            return Json(json!({"availableUserDomains": [".old.test"], "spacesAlpha": spaces}))
                .into_response();
        }
        "com.atproto.identity.resolveHandle" => return StatusCode::BAD_REQUEST.into_response(),
        _ => {}
    }
    if let Some(refused) = bound(&old, &headers, &nsid) {
        return refused;
    }
    let did = old.did();
    let octets =
        |bytes: Vec<u8>, kind: &str| ([("content-type", kind.to_string())], bytes).into_response();
    match nsid.as_str() {
        "com.atproto.server.getServiceAuth" => {
            assert_eq!(q["lxm"], "com.atproto.server.createAccount");
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let claims = Claims {
                iss: did,
                aud: q["aud"].clone(),
                exp: now + 60,
                iat: Some(now),
                lxm: Some(q["lxm"].clone()),
                jti: None,
            };
            Json(json!({"token": service_auth::sign(&old.key(), &claims)})).into_response()
        }
        // Two pages: the forum and an empty space of its own, then the
        // calendar of someone else's it writes in.
        "com.atproto.space.listSpaces" => Json(match q.get("cursor") {
            None => json!({"spaces": [{"space": old.forum()}, {"space": old.empty()}], "cursor": "p2"}),
            Some(_) => json!({"spaces": [{"space": "at://did:plc:elsewhere/space/com.example.calendar/main"}]}),
        })
        .into_response(),
        "com.atproto.space.exportSpaceMeta" => {
            let space = &q["space"];
            let meta = match *space == old.forum() {
                true => forum_meta(space),
                false => {
                    let personal = json!({"$type": "atproto-space-meta", "ver": 1, "space": space});
                    let drisl::Value::Map(m) = drisl::Value::from_json(&personal).unwrap() else {
                        panic!("a map")
                    };
                    let policy = encoded(json!({"$type": "com.atproto.personalspace.policy"}));
                    write_star(&m, [(&b"policy"[..], &policy[..])])
                }
            };
            octets(meta, "application/vnd.dasl.star")
        }
        "com.atproto.space.listBlobs" => Json(match q["space"] == old.forum() {
            true => json!({"cids": [Cid::of_raw(PHOTO).to_string()]}),
            false => json!({"cids": []}),
        })
        .into_response(),
        "com.atproto.space.getBlob" => octets(PHOTO.to_vec(), "image/png"),
        "com.atproto.space.getRepo" => match q["space"] == old.empty() {
            true => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "RepoNotFound", "message": "no repo there"})),
            )
                .into_response(),
            false => octets(space_repo(&did, &q["space"]), "application/vnd.dasl.star"),
        },
        "com.atproto.identity.requestPlcOperationSignature" => {
            assert!(body.is_empty(), "a body sent where none is taken");
            *old.asked_code.lock().unwrap() += 1;
            Json(json!({})).into_response()
        }
        "com.atproto.identity.signPlcOperation" => {
            let asked: Json_ = serde_json::from_slice(&body).unwrap();
            if asked["token"] != CODE {
                let no = json!({"error": "InvalidToken", "message": "that is not the code we sent"});
                return (StatusCode::BAD_REQUEST, Json(no)).into_response();
            }
            let strings = |v: &Json_| -> Vec<String> {
                v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
            };
            let op = Op {
                rotation_keys: strings(&asked["rotationKeys"]),
                verification_methods: asked["verificationMethods"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                    .collect(),
                also_known_as: strings(&asked["alsoKnownAs"]),
                services: BTreeMap::from([(
                    "atproto_pds".to_string(),
                    Service {
                        kind: "AtprotoPersonalDataServer".into(),
                        endpoint: asked["services"]["atproto_pds"]["endpoint"].as_str().unwrap().into(),
                    },
                )]),
            };
            let last = *old.last.lock().unwrap();
            let rotation = old.rotation.lock().unwrap().clone().unwrap();
            Json(json!({"operation": op.sign(last, &rotation).to_json()})).into_response()
        }
        other => panic!("no method {other} here"),
    }
}

/// A DID at the old server, made at the test directory: its DID, keys,
/// and its genesis.
async fn plc_account(h: &Running, old: &str, handle: &str) -> (String, Keypair, Keypair, Cid) {
    let key = Keypair::generate(Curve::K256);
    let rotation = Keypair::generate(Curve::K256);
    let op = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        handle,
        old,
    );
    let genesis = op.sign(None, &rotation);
    let did = genesis.did();
    let res = h
        .http
        .post(format!("{}/{did}", plc_url(h)))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    (did, key, rotation, genesis.cid())
}

/// A host, the old server, the account on it, and the person moving it,
/// signed in to the console: their DID and cookie.
struct World {
    h: Running,
    old: Arc<Old>,
    did: String,
    person: String,
    cookie: String,
}

fn browser() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn set_cookie(res: &reqwest::Response, name: &str) -> Option<String> {
    res.headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|c| c.starts_with(&format!("{name}=")))
        .map(|c| c.split(';').next().unwrap().to_string())
}

fn location(res: &reqwest::Response) -> String {
    res.headers()["location"].to_str().unwrap().to_string()
}

/// Goes through a sign-in started at `path` for `who`, with `cookie`: the
/// last answer.
async fn through(h: &Running, path: &str, who: &str, cookie: Option<&str>) -> reqwest::Response {
    let b = browser();
    let url = reqwest::Url::parse_with_params(
        &format!("{}{path}", h.url),
        &[("handle", who), ("back", "/move")],
    )
    .unwrap();
    let mut req = b.get(url);
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    let started = req.send().await.unwrap();
    if started.status() != 303 {
        return started;
    }
    let login = set_cookie(&started, "group_host_login").unwrap();
    let approved = b.get(location(&started)).send().await.unwrap();
    let cookies = [Some(login), cookie.map(String::from)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    b.get(location(&approved))
        .header("cookie", cookies.join("; "))
        .send()
        .await
        .unwrap()
}

async fn world() -> World {
    let old = Arc::new(Old::default());
    *old.first_lasts.lock().unwrap() = 3600;
    *old.spaces.lock().unwrap() = true;
    let router = Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource))
        .route("/.well-known/oauth-authorization-server", get(server))
        .route("/par", post(par))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/revoke", post(revoke))
        .route("/xrpc/{nsid}", get(xrpc).post(xrpc))
        .with_state(old.clone());
    let url = serve(router).await;
    *old.base.lock().unwrap() = url.clone();
    let h = start_reachable(&[("HANDLE_RESOLVERS", &url)]).await;
    let (did, key, rotation, last) = plc_account(&h, &url, "harbor.old.test").await;
    *old.did.lock().unwrap() = did.clone();
    *old.key.lock().unwrap() = Some(key);
    *old.rotation.lock().unwrap() = Some(rotation);
    *old.last.lock().unwrap() = Some(last);
    let (person, ..) = plc_account(&h, &url, "dana.old.test").await;
    let signed = through(&h, "/console/login", &person, None).await;
    let cookie = set_cookie(&signed, "group_host_session").unwrap();
    World {
        h,
        old,
        did,
        person,
        cookie,
    }
}

impl World {
    async fn call(&self, nsid: &str, body: Option<Json_>) -> (u16, Json_) {
        let url = format!("{}/xrpc/{NS}.{nsid}", self.h.url);
        let req = match body {
            Some(b) => self.h.http.post(url).json(&b),
            None => self.h.http.get(url),
        };
        let res = req.header("cookie", &self.cookie).send().await.unwrap();
        (res.status().as_u16(), res.json().await.unwrap_or_default())
    }

    async fn sign_in_to_move(&self) -> reqwest::Response {
        through(
            &self.h,
            "/console/move/login",
            &self.did,
            Some(&self.cookie),
        )
        .await
    }
}

fn error(said: &(u16, Json_)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

#[tokio::test]
async fn moves_an_account_signed_in_at_its_server_and_gives_its_tokens_back() {
    let w = world().await;
    // Nothing signed in to move yet.
    assert_eq!(w.call("getAccountMove", None).await, (200, json!({})));
    assert_eq!(
        error(&w.call("startAccountMove", Some(json!({}))).await),
        (400, "NotFound")
    );

    // Its server is asked for what a move needs, and the browser comes
    // back still signed in as the person moving it.
    let back = w.sign_in_to_move().await;
    assert_eq!(back.status(), 303);
    assert_eq!(location(&back), "/move");
    assert!(set_cookie(&back, "group_host_session").is_none());
    let asked = w.old.pushed.lock().unwrap().last().cloned().unwrap();
    let service = &w.h.host.config.service_did;
    // Percent-encoded whole: a did:web's port, already %3A, again.
    let aud = service.replace('%', "%25").replace(':', "%3A");
    assert_eq!(
        asked["scope"],
        format!(
            "atproto rpc:com.atproto.server.createAccount?aud={aud}%23opensocial identity:* space:*?authority=*&action=read_self"
        )
    );
    let (status, moving) = w.call("getAccountMove", None).await;
    assert_eq!(status, 200, "{moving}");
    assert_eq!(moving["did"], w.did.as_str());
    assert_eq!(moving["handle"], "harbor.old.test");
    assert_eq!(moving["pds"], w.old.base().as_str());
    assert_eq!(moving["started"], false);
    // Signed in again, the tokens before are given back.
    w.sign_in_to_move().await;
    // (Its own sign-in gave back the person's first, at1.)
    assert_eq!(*w.old.revoked.lock().unwrap(), ["at1", "at2", "rat2"]);

    // Started: the account copied, its spaces brought, and a code asked
    // for; the token lapses partway and is refreshed.
    w.old.lapsed.lock().unwrap().push("at3".into());
    let (status, started) = w
        .call("startAccountMove", Some(json!({"address": "harbor-club"})))
        .await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["handle"], "harbor-club.groups.test");
    assert_eq!(started["records"], 1);
    assert_eq!(
        started["spaces"],
        json!({"spaces": 3, "records": 2, "blobs": 1})
    );
    assert_eq!(*w.old.asked_code.lock().unwrap(), 1);
    let host = &w.h.host;
    assert!(
        host.store
            .space(&w.old.forum())
            .unwrap()
            .unwrap()
            .public_read
    );
    assert!(host.store.space(&w.old.empty()).unwrap().is_some());
    let calendar = "at://did:plc:elsewhere/space/com.example.calendar/main";
    assert_eq!(host.store.space_paths(&w.did, calendar).unwrap().len(), 1);
    assert_eq!(w.call("getAccountMove", None).await.1["started"], true);

    // The wrong code is its server's to refuse; the right one finishes it.
    let wrong = w
        .call("finishAccountMove", Some(json!({"code": "guess"})))
        .await;
    assert_eq!(error(&wrong), (502, "UpstreamFailure"));
    assert!(
        wrong.1["message"]
            .as_str()
            .unwrap()
            .contains("not the code we sent"),
        "{}",
        wrong.1
    );
    let (status, done) = w
        .call(
            "finishAccountMove",
            Some(json!({"code": format!(" {CODE} ")})),
        )
        .await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["handle"], "harbor-club.groups.test");
    assert!(host.deactivation(&w.did).unwrap().is_none());
    assert!(
        host.roles(&w.did, &w.person)
            .unwrap()
            .contains(&"admin".to_string())
    );
    let revoked = w.old.revoked.lock().unwrap().clone();
    assert!(
        revoked.ends_with(&["at4".to_string(), "rat4".to_string()]),
        "{revoked:?}"
    );
    assert_eq!(w.call("getAccountMove", None).await, (200, json!({})));
}

#[tokio::test]
async fn refreshes_a_token_about_to_lapse_and_says_what_its_server_refused() {
    let w = world().await;
    // A first token that lasts less than the host waits for: refreshed
    // before its first call. And a server without spaces, as most are
    // yet: it has none to bring.
    *w.old.first_lasts.lock().unwrap() = 10;
    *w.old.spaces.lock().unwrap() = false;
    w.sign_in_to_move().await;
    let (status, started) = w.call("startAccountMove", Some(json!({}))).await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(
        w.old.tokens.lock().unwrap().len(),
        3,
        "the person's, the account's, and its refresh"
    );
    assert_eq!(
        started["spaces"],
        json!({"spaces": 0, "records": 0, "blobs": 0})
    );
    let calls = w.old.calls.lock().unwrap().clone();
    assert!(
        !calls.iter().any(|c| c.starts_with("com.atproto.space.")),
        "{calls:?}"
    );
    // Finishing before starting, or with nothing signed in, is refused.
    assert_eq!(
        error(&w.call("cancelAccountMove", Some(json!({}))).await).0,
        200
    );
    assert!(
        w.h.host.store.account(&w.did).unwrap().is_none(),
        "the copy is discarded"
    );
    assert_eq!(
        error(
            &w.call("finishAccountMove", Some(json!({"code": CODE})))
                .await
        ),
        (400, "NotFound")
    );
    assert_eq!(
        error(&w.call("cancelAccountMove", Some(json!({}))).await),
        (400, "NotFound")
    );
    w.sign_in_to_move().await;
    assert_eq!(
        error(
            &w.call("finishAccountMove", Some(json!({"code": CODE})))
                .await
        ),
        (400, "NotStarted")
    );
    // Its server out of reach, the move says so.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let store = &w.h.host.store;
    let mut kept = store.move_of(&w.person, now).unwrap().unwrap();
    let nothing = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    kept.pds = format!("http://{}", nothing.local_addr().unwrap());
    drop(nothing);
    store.keep_move(&w.person, &kept, now).unwrap();
    let unreachable = w.call("startAccountMove", Some(json!({}))).await;
    assert_eq!(error(&unreachable), (502, "UpstreamFailure"));
    // Forgotten before it started, it leaves nothing to discard.
    assert_eq!(w.call("cancelAccountMove", Some(json!({}))).await.0, 200);
}

#[tokio::test]
async fn signs_in_to_move_only_for_someone_signed_in_to_the_console() {
    let w = world().await;
    let res = through(&w.h, "/console/move/login", &w.did, None).await;
    assert_eq!(res.status(), 401);
    assert!(
        res.text()
            .await
            .unwrap()
            .contains("sign in to the console first")
    );
    // Signed out between starting and coming back, it is refused.
    let b = browser();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/console/move/login", w.h.url),
        &[("handle", w.did.as_str())],
    )
    .unwrap();
    let started = b.get(url).header("cookie", &w.cookie).send().await.unwrap();
    let login = set_cookie(&started, "group_host_login").unwrap();
    let approved = b.get(location(&started)).send().await.unwrap();
    let back = b
        .get(location(&approved))
        .header("cookie", login)
        .send()
        .await
        .unwrap();
    assert_eq!(back.status(), 403);
    assert!(
        back.text()
            .await
            .unwrap()
            .contains("sign in to the console again first")
    );
    assert_eq!(w.call("getAccountMove", None).await, (200, json!({})));

    // An account whose document names no handle is shown by its DID
    // alone: no handle to show.
    let rotation = w.old.rotation.lock().unwrap().clone().unwrap();
    let mut op = Op::atproto(
        &w.old.key().public().did_key(),
        &[rotation.public().did_key()],
        "unnamed",
        &w.old.base(),
    );
    op.also_known_as.clear();
    let last = *w.old.last.lock().unwrap();
    let renamed =
        w.h.http
            .post(format!("{}/{}", plc_url(&w.h), w.did))
            .json(&op.sign(last, &rotation).to_json())
            .send()
            .await
            .unwrap();
    assert!(renamed.status().is_success());
    w.sign_in_to_move().await;
    let (status, moving) = w.call("getAccountMove", None).await;
    assert_eq!(status, 200, "{moving}");
    assert_eq!(moving["did"], w.did.as_str());
    assert_eq!(moving["pds"], w.old.base().as_str());
    assert_eq!(moving.get("handle"), None, "{moving}");
}

#[tokio::test]
async fn stops_rather_than_move_without_what_it_cannot_read_or_renew() {
    let w = world().await;
    w.sign_in_to_move().await;
    // Its server not saying whether it holds spaces: the move stops before
    // the identity moves, rather than leave them behind.
    *w.old.undescribed.lock().unwrap() = true;
    let said = w.call("startAccountMove", Some(json!({}))).await;
    assert_eq!(error(&said), (502, "UpstreamFailure"));
    assert_eq!(*w.old.asked_code.lock().unwrap(), 0);
    *w.old.undescribed.lock().unwrap() = false;
    // A token about to lapse with nothing to renew it by: sign in again.
    *w.old.no_refresh.lock().unwrap() = true;
    *w.old.first_lasts.lock().unwrap() = 10;
    w.sign_in_to_move().await;
    let said = w.call("startAccountMove", Some(json!({}))).await;
    assert_eq!(error(&said), (400, "SignInAgain"));
}

#[tokio::test]
async fn asks_once_for_a_nonce_its_server_never_gives() {
    let w = world().await;
    w.sign_in_to_move().await;
    // A server that asks for a nonce and never gives one is asked once
    // more, then taken at its word, rather than asked forever.
    *w.old.nonceless.lock().unwrap() = true;
    let started = w.call("startAccountMove", Some(json!({})));
    let said = tokio::time::timeout(std::time::Duration::from_secs(20), started)
        .await
        .expect("an answer, not a loop");
    assert_ne!(said.0, 200, "{}", said.1);
}

#[tokio::test]
async fn gives_back_and_forgets_a_move_kept_a_day() {
    let w = world().await;
    w.sign_in_to_move().await;
    let store = &w.h.host.store;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let kept = store.move_of(&w.person, now).unwrap().unwrap();
    // Not yet a day: kept.
    assert_eq!(w.h.host.sweep_moves(now).await.unwrap(), 0);
    let day = 24 * 60 * 60_000;
    store.keep_move(&w.person, &kept, now - day - 1).unwrap();
    assert_eq!(w.h.host.sweep_moves(now).await.unwrap(), 1);
    let revoked = w.old.revoked.lock().unwrap().clone();
    assert!(
        revoked.ends_with(&["at2".to_string(), "rat2".to_string()]),
        "{revoked:?}"
    );
    assert!(store.move_of(&w.person, now).unwrap().is_none());
}
