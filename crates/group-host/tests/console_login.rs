//! Signing in to the console as oneself, at one's own PDS, as a
//! confidential client; to several accounts in one browser, switching
//! between them; and signing out.

mod common;

use atproto::plc::Op;
use atproto::{Curve, Keypair, jose};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use common::groups::NS;
use common::spaces::plc_url;
use common::{Running, serve, start, start_reachable};
use serde_json::{Value as Json_, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A person's PDS and its authorization server, which approves at once,
/// each of its answers as `said` makes it.
#[derive(Default)]
struct Pds {
    base: Mutex<String>,
    said: Mutex<Json_>,
    pushed: Mutex<Vec<HashMap<String, String>>>,
    proofs: Mutex<Vec<String>>,
    revoked: Mutex<Vec<String>>,
}

impl Pds {
    fn says(&self, key: &str) -> Json_ {
        self.said.lock().unwrap()[key].clone()
    }
    fn base(&self) -> String {
        self.base.lock().unwrap().clone()
    }
}

async fn resource(State(pds): State<Arc<Pds>>) -> Json<Json_> {
    let servers = match pds.says("servers") {
        Json_::Null => json!([pds.base()]),
        other => other,
    };
    Json(json!({ "authorization_servers": servers }))
}

async fn server(State(pds): State<Arc<Pds>>) -> Json<Json_> {
    let base = pds.base();
    let issuer = pds
        .says("issuer")
        .as_str()
        .map_or(base.clone(), String::from);
    let mut meta = json!({
        "issuer": issuer,
        "pushed_authorization_request_endpoint": pds.says("par_at").as_str().map_or(format!("{base}/par"), String::from),
        "authorization_endpoint": pds.says("authorize").as_str().map_or(format!("{base}/authorize"), String::from),
        "token_endpoint": format!("{base}/token"),
    });
    if pds.says("revocation") != false {
        meta["revocation_endpoint"] = json!(format!("{base}/revoke"));
    }
    Json(meta)
}

/// Asks for a nonce first, as atproto's servers do.
fn nonce_first(pds: &Pds, headers: &HeaderMap) -> Option<Response> {
    let proof = headers["dpop"].to_str().unwrap().to_string();
    pds.proofs.lock().unwrap().push(proof.clone());
    let (_, claims) = jose::parts(&proof).unwrap();
    let fresh = claims["nonce"] == "n1" && pds.says("nonce") != "always";
    let ask = || {
        (
            StatusCode::BAD_REQUEST,
            [("dpop-nonce", "n1")],
            Json(json!({"error": "use_dpop_nonce"})),
        )
            .into_response()
    };
    (!fresh).then(ask)
}

async fn par(
    State(pds): State<Arc<Pds>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Some(ask) = nonce_first(&pds, &headers) {
        return ask;
    }
    pds.pushed.lock().unwrap().push(form);
    match pds.says("par").as_str() {
        Some("refuse") => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_request", "error_description": "no"})),
        )
            .into_response(),
        Some("silent") => (StatusCode::BAD_REQUEST, Json(json!({}))).into_response(),
        Some("empty") => Json(json!({})).into_response(),
        _ => Json(json!({"request_uri": "urn:ietf:params:oauth:request_uri:r1", "expires_in": 60}))
            .into_response(),
    }
}

async fn authorize(
    State(pds): State<Arc<Pds>>,
    Query(q): Query<HashMap<String, String>>,
) -> Redirect {
    let pushed = pds.pushed.lock().unwrap().last().cloned().unwrap();
    assert_eq!(q["request_uri"], "urn:ietf:params:oauth:request_uri:r1");
    let mut to = reqwest::Url::parse(&pushed["redirect_uri"]).unwrap();
    {
        let mut back = to.query_pairs_mut();
        match pds.says("approve").as_str() {
            Some("deny") => back.append_pair("error", "access_denied"),
            Some("bare") => back
                .append_pair("error", "x")
                .clear()
                .append_pair("nothing", "here"),
            _ => back.append_pair("code", "c1"),
        };
        back.append_pair("state", &pushed["state"]);
        // Unless asked to leave it out, as atproto's servers never may.
        if pds.says("iss") != false {
            let iss = pds.says("iss").as_str().map_or(pds.base(), String::from);
            back.append_pair("iss", &iss);
        }
    }
    Redirect::to(to.as_str())
}

async fn token(
    State(pds): State<Arc<Pds>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Some(ask) = nonce_first(&pds, &headers) {
        return ask;
    }
    assert_eq!(form["code"], "c1");
    if pds.says("token") == "refuse" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    }
    let sub = pds.says("sub");
    Json(json!({"access_token": "at", "refresh_token": "rt", "token_type": "DPoP", "sub": sub, "scope": "atproto", "expires_in": 60})).into_response()
}

async fn revoke(
    State(pds): State<Arc<Pds>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Some(ask) = nonce_first(&pds, &headers) {
        return ask;
    }
    pds.revoked.lock().unwrap().push(form["token"].clone());
    StatusCode::OK.into_response()
}

/// A person on a PDS of their own: their DID, and the PDS.
async fn person(h: &Running, with_pds: bool) -> (String, Arc<Pds>) {
    let pds = Arc::new(Pds::default());
    let router = Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource))
        .route("/.well-known/oauth-authorization-server", get(server))
        .route("/par", post(par))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/revoke", post(revoke))
        .with_state(pds.clone());
    let url = serve(router).await;
    *pds.base.lock().unwrap() = url.clone();
    let key = Keypair::generate(Curve::K256);
    let rotation = Keypair::generate(Curve::K256);
    let mut op = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        "person.test",
        &url,
    );
    if !with_pds {
        op.services.clear();
    }
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
    pds.said.lock().unwrap()["sub"] = json!(did);
    (did, pds)
}

/// A port nothing listens on.
fn closed() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}/par", listener.local_addr().unwrap())
}

fn browser() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// The `name=value` of the cookie a response sets, if it sets `name`.
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

/// Signs in as `who`, going back to `back`: the last answer, and the
/// cookie naming the sign-in.
async fn sign_in(h: &Running, who: &str, back: &str) -> (reqwest::Response, Option<String>) {
    sign_in_beside(h, who, back, "").await
}

/// Signs in as `who` in a browser holding the cookies `jar` (`;` between).
async fn sign_in_beside(
    h: &Running,
    who: &str,
    back: &str,
    jar: &str,
) -> (reqwest::Response, Option<String>) {
    let b = browser();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/console/login", h.url),
        &[("handle", who), ("back", back)],
    )
    .unwrap();
    let started = b.get(url).send().await.unwrap();
    if started.status() != 303 {
        return (started, None);
    }
    let login = set_cookie(&started, "group_host_login");
    let approved = b.get(location(&started)).send().await.unwrap();
    let back = b
        .get(location(&approved))
        .header("cookie", format!("{}; {jar}", login.clone().unwrap()))
        .send()
        .await
        .unwrap();
    (back, login)
}

async fn create_group(h: &Running, cookie: Option<&str>, handle: &str) -> (u16, Json_) {
    let mut req = h
        .http
        .post(format!("{}/xrpc/{NS}.createGroup", h.url))
        .json(&json!({"handle": handle, "displayName": "Riders"}));
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    let res = req.send().await.unwrap();
    (res.status().as_u16(), res.json().await.unwrap_or_default())
}

async fn text(res: reqwest::Response) -> String {
    res.text().await.unwrap()
}

#[tokio::test]
async fn signs_a_person_in_at_their_own_pds_and_out() {
    let h = start_reachable(&[]).await;
    let (did, pds) = person(&h, true).await;
    let (back, _) = sign_in(&h, &did, "/groups/riders").await;
    assert_eq!(back.status(), 303);
    assert_eq!(location(&back), "/groups/riders");
    let session = set_cookie(&back, "group_host_session").unwrap();
    let all: Vec<&str> = back
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(
        all.iter()
            .any(|c| c.contains("HttpOnly") && c.contains("SameSite=Lax")),
        "{all:?}"
    );
    assert!(
        all.iter().any(|c| c.starts_with("group_host_login=;")),
        "{all:?}"
    );

    // What it asked for: atproto alone, as a confidential client, with
    // PKCE and DPoP, naming who signs in.
    let pushed = pds.pushed.lock().unwrap().last().cloned().unwrap();
    assert_eq!(pushed["scope"], "atproto");
    assert_eq!(pushed["login_hint"], did);
    assert_eq!(pushed["code_challenge_method"], "S256");
    assert_eq!(
        pushed["client_id"],
        format!("{}/console/client-metadata.json", h.url)
    );
    let (_, meta) = h.get_path("/console/client-metadata.json").await;
    assert_eq!(meta["client_id"], pushed["client_id"]);
    assert_eq!(
        meta["redirect_uris"],
        json!([format!("{}/console/callback", h.url)])
    );
    let key = jose::from_jwk(&meta["jwks"]["keys"][0]).unwrap();
    let (header, claims) = jose::verify(&pushed["client_assertion"], &key).unwrap();
    assert_eq!(header["kid"], "console");
    assert_eq!(
        (claims["iss"].clone(), claims["aud"].clone()),
        (json!(pushed["client_id"]), json!(pds.base()))
    );
    // And it gave back what it was given.
    assert_eq!(*pds.revoked.lock().unwrap(), ["at"]);

    // The cookie is who calls the host's methods.
    let (status, made) = create_group(&h, Some(&session), "riders.groups.test").await;
    assert_eq!(status, 200, "{made}");
    assert!(
        h.host
            .roles(made["did"].as_str().unwrap(), &did)
            .unwrap()
            .contains(&"admin".to_string())
    );
    let forged = format!("{}x", session);
    assert_eq!(
        create_group(&h, Some(&forged), "other.groups.test").await.0,
        401
    );
    assert_eq!(
        create_group(&h, Some("group_host_session=nothing"), "other.groups.test")
            .await
            .0,
        401
    );

    // Signing out ends it, and every cookie signed before.
    let out = browser()
        .post(format!("{}/console/logout", h.url))
        .header("cookie", &session)
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), 303);
    assert_eq!(
        set_cookie(&out, "group_host_session").unwrap(),
        "group_host_session="
    );
    assert_eq!(
        create_group(&h, Some(&session), "other.groups.test")
            .await
            .0,
        401
    );
    let again = browser()
        .post(format!("{}/console/logout", h.url))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 303);
    // Signing in again is a new one.
    let (back, _) = sign_in(&h, &did, "//elsewhere.test").await;
    assert_eq!(location(&back), "/");
    let session = set_cookie(&back, "group_host_session").unwrap();
    assert_eq!(
        create_group(&h, Some(&session), "hikers.groups.test")
            .await
            .0,
        200
    );
}

#[tokio::test]
async fn refuses_a_sign_in_it_cannot_trust() {
    let h = start_reachable(&[]).await;
    let (did, pds) = person(&h, true).await;
    let set = |k: &str, v: Json_| pds.said.lock().unwrap()[k] = v;
    let refused = async |who: &str| {
        let (res, _) = sign_in(&h, who, "/").await;
        let status = res.status().as_u16();
        (status, text(res).await)
    };
    let (_, no_pds) = person(&h, false).await;
    drop(no_pds);
    for (case, value, said) in [
        ("servers", json!([]), "names no authorization server"),
        ("issuer", json!("https://elsewhere.test"), "calls itself"),
        ("par", json!("refuse"), "refused: no"),
        ("par", json!("silent"), "refused: no reason given"),
        ("par", json!("empty"), "gave no request_uri"),
        (
            "authorize",
            json!("not a url"),
            "names no authorization endpoint",
        ),
        ("nonce", json!("always"), "asked for a nonce twice"),
        ("par_at", json!(closed()), "error sending request"),
        (
            "iss",
            json!("https://elsewhere.test"),
            "came from another server",
        ),
        ("iss", json!(false), "did not say which server"),
        ("approve", json!("deny"), "it was refused: access_denied"),
        ("approve", json!("bare"), "it was refused: no reason given"),
        ("token", json!("refuse"), "refused: invalid_grant"),
        (
            "sub",
            json!("did:plc:someone"),
            "signed in as \"did:plc:someone\"",
        ),
    ] {
        let before = pds.said.lock().unwrap().clone();
        set(case, value);
        let (status, why) = refused(&did).await;
        assert!(status >= 400, "{case}: {status}");
        assert!(why.contains(said), "{case}: {why}");
        *pds.said.lock().unwrap() = before;
    }
    // Who it cannot find, or who has no PDS.
    for (who, said) in [
        ("nobody.invalid", "no account we can find"),
        ("  ", "say who you are"),
    ] {
        let (status, why) = refused(who).await;
        assert_eq!(status, 400);
        assert!(why.contains(said), "{who}: {why}");
    }
    let no_pds_person = {
        let key = Keypair::generate(Curve::K256);
        let mut op = Op::atproto(
            &key.public().did_key(),
            &[key.public().did_key()],
            "x.test",
            "https://x.test",
        );
        op.services.clear();
        let genesis = op.sign(None, &key);
        h.http
            .post(format!("{}/{}", plc_url(&h), genesis.did()))
            .json(&genesis.to_json())
            .send()
            .await
            .unwrap();
        genesis.did()
    };
    let (_, why) = refused(&no_pds_person).await;
    assert!(why.contains("names no PDS"), "{why}");
    let (_, why) = refused("did:plc:unknownunknownunknown").await;
    assert!(why.contains("did:plc:unknownunknownunknown"), "{why}");

    // An answer in a browser that did not start it, or none at all.
    let b = browser();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/console/login", h.url),
        &[("handle", did.as_str())],
    )
    .unwrap();
    let started = b.get(url).send().await.unwrap();
    let approved = b.get(location(&started)).send().await.unwrap();
    let back = b
        .get(location(&approved))
        .header("cookie", "group_host_login=other")
        .send()
        .await
        .unwrap();
    assert_eq!(back.status(), 400);
    let back = b.get(location(&approved)).send().await.unwrap();
    assert_eq!(back.status(), 400);
    // A server without revocation is not asked to revoke.
    set("revocation", json!(false));
    let (back, _) = sign_in(&h, &did, "/").await;
    assert_eq!(back.status(), 303);
    assert_eq!(*pds.revoked.lock().unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn reaches_only_public_servers_for_a_host_on_the_internet() {
    let h = start(&[("CONSOLE_URL", "https://console.test")], false).await;
    let (did, _) = person(&h, true).await;
    let (res, _) = sign_in(&h, &did, "/").await;
    assert_eq!(res.status(), 400);
    let why = text(res).await;
    assert!(why.contains("not an https URL"), "{why}");
    let (_, meta) = h.get_path("/console/client-metadata.json").await;
    assert_eq!(
        meta["client_id"],
        "https://console.test/console/client-metadata.json"
    );
    // Its cookies are for https alone.
    let out = browser()
        .post(format!("{}/console/logout", h.url))
        .send()
        .await
        .unwrap();
    let set = out.headers()["set-cookie"].to_str().unwrap();
    assert!(set.ends_with("; Secure"), "{set}");
}

/// A PDS and its authorization server, standing alone: for a host whose
/// `SIGNUP_PDS` it is, made before the host is.
async fn signup_pds() -> Arc<Pds> {
    let pds = Arc::new(Pds::default());
    let router = Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource))
        .route("/.well-known/oauth-authorization-server", get(server))
        .route("/par", post(par))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/revoke", post(revoke))
        .with_state(pds.clone());
    *pds.base.lock().unwrap() = serve(router).await;
    pds
}

/// An account at `pds`, as signing up there makes one: its DID.
async fn account_at(h: &Running, pds: &str) -> String {
    let key = Keypair::generate(Curve::K256);
    let rotation = Keypair::generate(Curve::K256);
    let op = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        "new.test",
        pds,
    );
    let genesis = op.sign(None, &rotation);
    let url = format!("{}/{}", plc_url(h), genesis.did());
    let res = h
        .http
        .post(url)
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    genesis.did()
}

/// Signs someone up from the console, going back to `back`: the last
/// answer.
async fn sign_up(h: &Running, back: &str) -> reqwest::Response {
    let b = browser();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/console/login", h.url),
        &[("create", ""), ("back", back)],
    )
    .unwrap();
    let started = b.get(url).send().await.unwrap();
    if started.status() != 303 {
        return started;
    }
    let login = set_cookie(&started, "group_host_login").unwrap();
    let approved = b.get(location(&started)).send().await.unwrap();
    b.get(location(&approved))
        .header("cookie", login)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn signs_someone_up_at_the_sign_up_pds() {
    let pds = signup_pds().await;
    let h = start_reachable(&[("SIGNUP_PDS", &format!("{}/", pds.base()))]).await;
    assert_eq!(h.host.config.signup_pds, pds.base());
    let did = account_at(&h, &pds.base()).await;
    pds.said.lock().unwrap()["sub"] = json!(did);

    let back = sign_up(&h, "/new").await;
    assert_eq!(
        (back.status().as_u16(), location(&back)),
        (303, "/new".to_string())
    );
    // Asked to make an account, for no one in particular.
    let pushed = pds.pushed.lock().unwrap().last().cloned().unwrap();
    assert_eq!(pushed["prompt"], "create");
    assert!(!pushed.contains_key("login_hint"));
    let session = set_cookie(&back, COOKIE_NAME).unwrap();
    let (status, view) = viewer(&h, &session).await;
    assert_eq!((status, view["actor"]["did"].clone()), (200, json!(did)));
    assert_eq!(view["signUpAt"], pds.base());
}

#[tokio::test]
async fn refuses_a_sign_up_it_cannot_trust() {
    let pds = signup_pds().await;
    let h = start_reachable(&[("SIGNUP_PDS", &pds.base())]).await;
    // An account another server answers for, though this one said it made it.
    let (elsewhere, _other) = person(&h, true).await;
    pds.said.lock().unwrap()["sub"] = json!(elsewhere);
    let res = sign_up(&h, "/").await;
    assert_eq!(res.status().as_u16(), 403);
    assert!(text(res).await.contains("signs in at"));
    // No account at all.
    pds.said.lock().unwrap()["sub"] = json!("");
    let res = sign_up(&h, "/").await;
    assert_eq!(res.status().as_u16(), 403);

    // A sign-up PDS that cannot be reached.
    let away = start_reachable(&[("SIGNUP_PDS", "http://127.0.0.1:9")]).await;
    let res = sign_up(&away, "/").await;
    assert_eq!(res.status().as_u16(), 400);
    // Neither who they are nor that they are new.
    let res = browser()
        .get(format!("{}/console/login", h.url))
        .send()
        .await
        .unwrap();
    assert!(text(res).await.contains("say who you are"));
}

const COOKIE_NAME: &str = "group_host_session";

async fn viewer(h: &Running, cookie: &str) -> (u16, Json_) {
    let res = h
        .http
        .get(format!("{}/xrpc/{NS}.getViewer", h.url))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    (res.status().as_u16(), res.json().await.unwrap_or_default())
}

/// The cookies a browser holds, `;` between, once `res` set them over
/// `jar`: a cookie set empty is gone, and the sign-in's own is left out.
fn kept(jar: &str, res: &reqwest::Response) -> String {
    let mut all: Vec<(String, String)> = jar
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    for set in res.headers().get_all("set-cookie") {
        let pair = set.to_str().unwrap().split(';').next().unwrap();
        let (name, value) = pair.split_once('=').unwrap();
        all.retain(|(k, _)| k != name);
        if !value.is_empty() && name != "group_host_login" {
            all.push((name.to_string(), value.to_string()));
        }
    }
    let pairs: Vec<String> = all.iter().map(|(k, v)| format!("{k}={v}")).collect();
    pairs.join("; ")
}

async fn switch(h: &Running, jar: &str, did: &str, back: &str) -> reqwest::Response {
    browser()
        .post(format!("{}/console/switch", h.url))
        .header("cookie", jar)
        .form(&[("did", did), ("back", back)])
        .send()
        .await
        .unwrap()
}

async fn sign_out(h: &Running, jar: &str) -> reqwest::Response {
    browser()
        .post(format!("{}/console/logout", h.url))
        .header("cookie", jar)
        .send()
        .await
        .unwrap()
}

/// The DIDs of the other accounts the viewer is signed in to.
fn others(viewer: &Json_) -> Vec<String> {
    let all = viewer["accounts"].as_array().cloned().unwrap_or_default();
    all.iter()
        .map(|a| a["did"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn switches_between_accounts_signed_in_in_one_browser() {
    let h = start_reachable(&[]).await;
    let (one, _) = person(&h, true).await;
    let (two, _) = person(&h, true).await;
    let (three, _) = person(&h, true).await;

    // One account: nothing else to switch to.
    let (back, _) = sign_in(&h, &one, "/").await;
    assert!(set_cookie(&back, "group_host_accounts").is_none());
    let first = kept("", &back);
    let (_, shown) = viewer(&h, &first).await;
    assert_eq!(shown["actor"]["did"], one);
    assert_eq!(shown.get("accounts"), None);

    // Signing in to another keeps the first beside it, and the same one
    // again keeps it once.
    let (back, _) = sign_in_beside(&h, &two, "/g/riders", &first).await;
    assert_eq!(location(&back), "/g/riders");
    let both = kept(&first, &back);
    let (_, shown) = viewer(&h, &both).await;
    assert_eq!(shown["actor"]["did"], two);
    assert_eq!(others(&shown), [one.as_str()]);
    assert_eq!(shown["accounts"][0]["handle"], "person.test");
    let (back, _) = sign_in_beside(&h, &two, "/", &both).await;
    let again = kept(&both, &back);
    assert_eq!(others(&viewer(&h, &again).await.1), [one.as_str()]);
    // Signing in to one already beside makes it the one in use, once.
    let (back, _) = sign_in_beside(&h, &one, "/", &both).await;
    let flipped = kept(&both, &back);
    let (_, shown) = viewer(&h, &flipped).await;
    assert_eq!(shown["actor"]["did"], one);
    assert_eq!(others(&shown), [two.as_str()]);

    // Switching swaps them, with no sign-in, going back to the page it
    // was asked from; only to an account this browser is signed in to.
    let res = switch(&h, &both, &one, "/g/riders").await;
    assert_eq!(res.status(), 303);
    assert_eq!(location(&res), "/g/riders");
    let swapped = kept(&both, &res);
    let (_, shown) = viewer(&h, &swapped).await;
    assert_eq!(shown["actor"]["did"], one);
    assert_eq!(others(&shown), [two.as_str()]);
    let res = switch(&h, &first, &two, "//elsewhere.test").await;
    assert_eq!(res.status(), 400);
    let res = switch(&h, "", &two, "/").await;
    assert_eq!(res.status(), 400);
    // Elsewhere is never where it goes back to.
    let res = switch(&h, &swapped, &two, "//elsewhere.test").await;
    assert_eq!(location(&res), "/");

    // Three: the one used last first.
    let (back, _) = sign_in_beside(&h, &three, "/", &swapped).await;
    let all = kept(&swapped, &back);
    let (_, shown) = viewer(&h, &all).await;
    assert_eq!(shown["actor"]["did"], three);
    assert_eq!(others(&shown), [one.as_str(), two.as_str()]);

    // Signing out of one goes on as the one used before it, and that one
    // is never offered again by the cookies signed before.
    let res = sign_out(&h, &all).await;
    assert_eq!(res.status(), 303);
    let left = kept(&all, &res);
    let (_, shown) = viewer(&h, &left).await;
    assert_eq!(shown["actor"]["did"], one);
    assert_eq!(others(&shown), [two.as_str()]);
    assert_eq!(others(&viewer(&h, &swapped).await.1), [two.as_str()]);
    let res = switch(&h, &all, &three, "/").await;
    assert_eq!(res.status(), 400);
    // A browser whose account in use was signed out still switches to one
    // beside it, and keeps nothing of the one signed out.
    let res = switch(&h, &all, &one, "/").await;
    assert_eq!(res.status(), 303);
    let (_, shown) = viewer(&h, &kept(&all, &res)).await;
    assert_eq!(shown["actor"]["did"], one);
    assert_eq!(others(&shown), [two.as_str()]);

    // Out of the next, and the last, as no one.
    let res = sign_out(&h, &left).await;
    let last = kept(&left, &res);
    assert!(!last.contains("group_host_accounts="), "{last}");
    let (_, shown) = viewer(&h, &last).await;
    assert_eq!(shown["actor"]["did"], two);
    assert_eq!(shown.get("accounts"), None);
    let res = sign_out(&h, &last).await;
    let none = kept(&last, &res);
    assert_eq!(viewer(&h, &none).await.1.get("actor"), None);
}

#[tokio::test]
async fn a_session_is_the_consoles_alone_and_only_its_pages_use_it() {
    // The console on https, as deployed: its cookies named `__Host-`.
    let console = "https://console.test";
    let h = start_reachable(&[("CONSOLE_URL", console)]).await;
    let (did, _) = person(&h, true).await;
    let value = h.host.session_cookie(&did, group_host::time::now_millis());
    let name = h.host.cookie_name(COOKIE_NAME);
    assert_eq!(name, "__Host-group_host_session");
    let jar = format!("{name}={value}");
    let as_from = async |origin: Option<&str>| {
        let mut req = h
            .http
            .get(format!("{}/xrpc/{NS}.getViewer", h.url))
            .header("cookie", &jar);
        if let Some(o) = origin {
            req = req.header("origin", o);
        }
        let said: Json_ = req.send().await.unwrap().json().await.unwrap();
        said["actor"]["did"].as_str().map(String::from)
    };
    // From the console's own pages, or with no word of where from.
    assert_eq!(as_from(None).await.as_deref(), Some(did.as_str()));
    assert_eq!(as_from(Some(console)).await.as_deref(), Some(did.as_str()));
    // From a page anywhere else, a group's site or a sandboxed page, it is
    // as nobody's.
    for elsewhere in [
        "https://riders.groups.test",
        "null",
        "https://console.test:8443",
    ] {
        assert_eq!(as_from(Some(elsewhere)).await, None, "{elsewhere}");
    }
    // The name before `__Host-` is nobody's here.
    let (_, plain) = viewer(&h, &format!("{COOKIE_NAME}={value}")).await;
    assert_eq!(plain.get("actor"), None);

    // Nor does a page elsewhere sign anyone in to another account, or out.
    for path in ["/console/switch", "/console/logout"] {
        let res = browser()
            .post(format!("{}{path}", h.url))
            .header("cookie", &jar)
            .header("origin", "https://riders.groups.test")
            .form(&[("did", did.as_str())])
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403, "{path}");
        assert_eq!(res.headers().get("set-cookie"), None, "{path}");
    }
    assert_eq!(as_from(None).await.as_deref(), Some(did.as_str()));
    // The console's own page signs out, the cookie cleared as it was set.
    let out = browser()
        .post(format!("{}/console/logout", h.url))
        .header("cookie", &jar)
        .header("origin", console)
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), 303);
    assert_eq!(
        out.headers()["set-cookie"],
        "__Host-group_host_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; Secure"
    );
    assert_eq!(as_from(None).await, None);
}
