//! The OAuth server apps sign in to a group with: its metadata, pushed
//! requests and their DPoP proofs, the browser's way to the console, and
//! codes and refresh tokens exchanged for tokens, then revoked.

mod common;

use atproto::jose;
use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use serde_json::{Value as Json, json};

const SCOPE: &str = "atproto repo:app.bsky.feed.post";

/// `who` made a member of `did` with `roles`, as admitting them would.
async fn join(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let membership = json!({"$type": format!("{NS}.membership"), "member": who, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let write = group_host::repo::Write::Create {
        collection: format!("{NS}.membership"),
        rkey: who.into(),
        value: membership,
    };
    let space = group_host::group::members_space(did);
    h.host.space_write(did, &space, &[write]).await.unwrap();
}

async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    (did, [alice, bob, carol])
}

#[tokio::test]
async fn says_so_when_it_fails_itself() {
    let h = start(&[], false).await;
    age(&h, "DROP TABLE oauth_request");
    let app = App::loopback(SCOPE);
    let said = app.push_said(&h).await;
    assert_eq!(said.error(), (500, "server_error"));
    assert!(said.headers.contains_key("dpop-nonce"));
}

#[tokio::test]
async fn says_what_it_is() {
    let h = start(&[], false).await;
    let (_, server) = h.get_path("/.well-known/oauth-authorization-server").await;
    assert_eq!(server["issuer"], "https://host.test");
    assert_eq!(
        server["pushed_authorization_request_endpoint"],
        "https://host.test/oauth/par"
    );
    assert_eq!(server["require_pushed_authorization_requests"], true);
    assert_eq!(
        server["dpop_signing_alg_values_supported"],
        json!(["ES256", "ES256K"])
    );
    let (_, resource) = h.get_path("/.well-known/oauth-protected-resource").await;
    assert_eq!(
        resource["authorization_servers"],
        json!(["https://host.test"])
    );
}

#[tokio::test]
async fn lets_a_page_anywhere_reach_it() {
    let h = start(&[], false).await;
    let res = h
        .http
        .request(reqwest::Method::OPTIONS, format!("{}/oauth/par", h.url))
        .header("origin", "https://app.test")
        .header("access-control-request-method", "POST")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    assert_eq!(res.headers()["access-control-allow-origin"], "*");
    let allowed = res.headers()["access-control-allow-headers"]
        .to_str()
        .unwrap();
    assert!(allowed.contains("dpop"), "{allowed}");
    let said = App::loopback(SCOPE).post(&h, "/oauth/par", &[]).await;
    let exposed = said.headers["access-control-expose-headers"]
        .to_str()
        .unwrap();
    assert!(exposed.contains("dpop-nonce"), "{exposed}");
}

#[tokio::test]
async fn asks_for_a_nonce_then_takes_the_request() {
    let h = start(&[], false).await;
    let app = App::loopback(SCOPE);
    let challenge = jose::sha256(&app.verifier);
    let form = app.request(&challenge, &[]);
    let first = app
        .send(
            &h,
            "/oauth/par",
            &form,
            Some(app.proof_with(&h, "POST", "/oauth/par", |_| {})),
        )
        .await;
    assert_eq!(first.error(), (400, "use_dpop_nonce"));
    assert!(first.headers.contains_key("dpop-nonce"));
    assert_eq!(first.headers["cache-control"], "no-store");
    let said = app.post(&h, "/oauth/par", &form).await;
    assert_eq!(said.status, 201, "{}", said.body);
    let uri = said.body["request_uri"].as_str().unwrap();
    assert!(
        uri.starts_with("urn:ietf:params:oauth:request_uri:req-"),
        "{uri}"
    );
    assert_eq!(said.body["expires_in"], 299);
}

#[tokio::test]
async fn refuses_a_request_it_cannot_take() {
    let h = start(&[], false).await;
    let app = App::loopback(SCOPE);
    let challenge = jose::sha256(&app.verifier);
    let other_key = jose::thumbprint(&atproto::Keypair::generate(atproto::Curve::P256).public());
    for (form, refused) in [
        (vec![("client_id", "")], "invalid_request"),
        (vec![("request", "eyJ9")], "request_not_supported"),
        (
            vec![("response_type", "token")],
            "unsupported_response_type",
        ),
        (
            vec![("redirect_uri", "https://elsewhere.test/")],
            "invalid_request",
        ),
        (vec![("code_challenge", "\0")], "invalid_request"),
        (vec![("code_challenge_method", "plain")], "invalid_request"),
        (vec![("response_mode", "form_post")], "invalid_request"),
        (
            vec![("dpop_jkt", other_key.as_str())],
            "invalid_dpop_key_binding",
        ),
        (vec![("scope", "repo:app.bsky.feed.post")], "invalid_scope"),
        (vec![("redirect_uri", "nonsense")], "invalid_request"),
        (
            vec![("scope", "atproto transition:generic")],
            "invalid_scope",
        ),
    ] {
        let said = app
            .post(&h, "/oauth/par", &app.request(&challenge, &form))
            .await;
        assert_eq!(said.error(), (400, refused), "{form:?}: {}", said.body);
    }
    // Without a proof, and with a body that is no form.
    let said = app
        .send(&h, "/oauth/par", &app.request(&challenge, &[]), None)
        .await;
    assert_eq!(said.error(), (400, "invalid_dpop_proof"));
    let res = h
        .http
        .post(format!("{}/oauth/par", h.url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Json = res.json().await.unwrap();
    assert_eq!(body["error"], "invalid_request");
}

#[tokio::test]
async fn takes_the_one_redirect_a_client_registered_and_its_own_scope() {
    let h = start(&[], false).await;
    let app = App::loopback(SCOPE);
    let challenge = jose::sha256(&app.verifier);
    let form = app.request(&challenge, &[("scope", "\0")]);
    let said = app.post(&h, "/oauth/par", &form).await;
    assert_eq!(said.status, 201, "{}", said.body);
    // At a loopback address, on any port.
    let at = "http://127.0.0.1:5173/callback";
    let said = app
        .post(
            &h,
            "/oauth/par",
            &app.request(&challenge, &[("redirect_uri", at)]),
        )
        .await;
    assert_eq!(said.status, 201, "{}", said.body);
}

#[tokio::test]
async fn checks_each_proof() {
    let h = start(&[], false).await;
    let app = App::loopback(SCOPE);
    let challenge = jose::sha256(&app.verifier);
    let form = app.request(&challenge, &[]);
    app.post(&h, "/oauth/par", &form).await;
    let path = "/oauth/par";
    let proof = |change: &dyn Fn(&mut Json)| app.proof_with(&h, "POST", path, change);
    let reused = proof(&|_| {});
    let first = app.send(&h, path, &form, Some(reused.clone())).await;
    assert_eq!(first.status, 201, "{}", first.body);
    let header = json!({"typ": "dpop+jwt", "jwk": jose::jwk(&app.key.public())});
    let other = atproto::Keypair::generate(atproto::Curve::P256);
    for (sent, refused) in [
        (reused, "used before"),
        ("not.a.jwt".to_string(), "bad JSON"),
        (
            jose::sign(
                &app.key,
                &json!({"typ": "JWT", "jwk": header["jwk"]}),
                &json!({}),
            ),
            "typed dpop+jwt",
        ),
        (
            jose::sign(&app.key, &json!({"typ": "dpop+jwt"}), &json!({})),
            "not an EC key",
        ),
        (jose::sign(&other, &header, &json!({})), "bad signature"),
        (proof(&|c| c["jti"] = json!("")), "without a jti"),
        (proof(&|c| c["htm"] = json!("GET")), "htm"),
        (
            proof(&|c| c["htu"] = json!("https://elsewhere.test/oauth/par")),
            "htu",
        ),
        (proof(&|c| c["iat"] = json!(now() - 600)), "too old"),
        (proof(&|c| c["iat"] = json!(now() + 600)), "from the future"),
    ] {
        let said = app.send(&h, path, &form, Some(sent)).await;
        assert_eq!(said.status, 400);
        let why = said.body["error_description"].as_str().unwrap();
        assert!(why.contains(refused), "{refused}: {why}");
    }
    let said = app
        .send(
            &h,
            path,
            &form,
            Some(proof(&|c| c["nonce"] = json!("stale"))),
        )
        .await;
    assert_eq!(said.error(), (400, "use_dpop_nonce"));
    // The proof's own query and fragment are not the request's to match.
    let said = app
        .send(
            &h,
            path,
            &form,
            Some(proof(&|c| {
                c["htu"] = json!("https://host.test/oauth/par?x#y")
            })),
        )
        .await;
    assert_eq!(said.status, 201, "{}", said.body);
    // One proof, not two.
    let res = h
        .http
        .post(format!("{}{path}", h.url))
        .form(&[("client_id", app.id.as_str())])
        .header("dpop", proof(&|_| {}))
        .header("dpop", proof(&|_| {}))
        .send()
        .await
        .unwrap();
    let body: Json = res.json().await.unwrap();
    assert_eq!(body["error_description"], "one DPoP proof, not several");
}

#[tokio::test]
async fn sends_the_browser_on_to_the_console() {
    let h = start(&[("CONSOLE_URL", "https://console.test/")], false).await;
    let app = App::loopback(SCOPE);
    let request_uri = app.push(&h, &[]).await;
    let no_redirects = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let go = |client_id: String, uri: String| {
        let url = reqwest::Url::parse_with_params(
            &format!("{}/oauth/authorize", h.url),
            &[("client_id", client_id), ("request_uri", uri)],
        )
        .unwrap();
        no_redirects.get(url).send()
    };
    let res = go(app.id.clone(), request_uri.clone()).await.unwrap();
    assert_eq!(res.status(), 303);
    let to = res.headers()["location"].to_str().unwrap();
    assert!(
        to.starts_with("https://console.test/authorize?request_uri=urn"),
        "{to}"
    );
    assert_eq!(param(to, "request_uri").unwrap(), request_uri);
    // Not for another client, nor a request that never was.
    let res = go("http://localhost".into(), request_uri).await.unwrap();
    assert_eq!(res.status(), 400);
    let res = go(app.id.clone(), "req-nothing".into()).await.unwrap();
    assert_eq!(res.status(), 400);
    assert!(res.text().await.unwrap().contains("Go back to the app"));
}

#[tokio::test]
async fn gives_a_token_for_the_group_with_what_the_member_may_do() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let app = App::loopback(SCOPE);
    let tokens = app.sign_in(&h, &alice, &did).await;
    assert_eq!(tokens["token_type"], "DPoP");
    assert_eq!(tokens["sub"], did.as_str());
    assert_eq!(tokens["scope"], SCOPE);
    assert_eq!(tokens["expires_in"], 3600);
    assert_ne!(tokens["access_token"], tokens["refresh_token"]);
}

#[tokio::test]
async fn sends_the_code_back_with_the_state_and_this_issuer() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let app = App::loopback(SCOPE);
    let request_uri = app.push(&h, &[]).await;
    let (_, approved) = approve(&h, &alice, &request_uri, &did).await;
    let to = approved["redirectUrl"].as_str().unwrap();
    assert!(to.starts_with(&format!("{REDIRECT}?code=cod-")), "{to}");
    assert_eq!(param(to, "state").unwrap(), "st4te");
    assert_eq!(param(to, "iss").unwrap(), "https://host.test");
    // Or in the fragment, if the app asked.
    let request_uri = app
        .push(&h, &[("response_mode", "fragment"), ("state", "\0")])
        .await;
    let (_, approved) = approve(&h, &alice, &request_uri, &did).await;
    let to = approved["redirectUrl"].as_str().unwrap();
    assert!(to.starts_with(&format!("{REDIRECT}#code=cod-")), "{to}");
    assert_eq!(param(to, "state"), None);
}

#[tokio::test]
async fn takes_a_code_once_from_its_own_client_and_key() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let app = App::loopback(SCOPE);
    let code_for = async |app: &App| {
        let request_uri = app.push(&h, &[]).await;
        let (_, approved) = approve(&h, &alice, &request_uri, &did).await;
        approved["redirectUrl"].as_str().unwrap().to_string()
    };
    let to = code_for(&app).await;
    assert_eq!(app.exchange(&h, &to).await.status, 200);
    assert_eq!(app.exchange(&h, &to).await.error(), (400, "invalid_grant"));

    let code = |to: &str| param(to, "code").unwrap();
    let exchange = async |app: &App, to: &str, change: &[(&str, &str)]| {
        let code = code(to);
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", REDIRECT),
            ("code_verifier", app.verifier.as_str()),
        ];
        for (k, v) in change {
            form.retain(|(f, _)| f != k);
            form.push((k, v));
        }
        app.post(&h, "/oauth/token", &form).await
    };
    let to = code_for(&app).await;
    let said = exchange(&app, &to, &[("code_verifier", "wrong")]).await;
    assert_eq!(said.error(), (400, "invalid_grant"));
    // Refused once, it is gone.
    assert_eq!(
        exchange(&app, &to, &[]).await.error(),
        (400, "invalid_grant")
    );
    for change in [
        vec![("code_verifier", "\0")],
        vec![("code_verifier", "!".repeat(43).leak())],
        vec![("redirect_uri", "http://127.0.0.1/other")],
        vec![("code", "\0")],
    ] {
        let to = code_for(&app).await;
        let said = exchange(&app, &to, &change).await;
        assert_eq!(said.status, 400, "{change:?}: {}", said.body);
    }
    // Another key, or another client.
    let to = code_for(&app).await;
    let thief = App::new(&app.id);
    let said = exchange(&thief, &to, &[("code_verifier", app.verifier.as_str())]).await;
    assert_eq!(said.error(), (400, "invalid_dpop_key_binding"));
    let to = code_for(&app).await;
    let other = App::loopback("atproto");
    let said = exchange(&other, &to, &[("code_verifier", app.verifier.as_str())]).await;
    assert_eq!(said.error(), (400, "invalid_grant"));
    // Nor once it has lapsed.
    let to = code_for(&app).await;
    age(&h, "UPDATE oauth_request SET expires_at = 1");
    let said = exchange(&app, &to, &[]).await;
    assert_eq!(said.body["error_description"], "the code has lapsed");
    let said = app
        .post(&h, "/oauth/token", &[("grant_type", "password")])
        .await;
    assert_eq!(said.error(), (400, "unsupported_grant_type"));
}

#[tokio::test]
async fn refreshes_into_a_new_pair_and_the_old_stops_working() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let app = App::loopback(SCOPE);
    let first = app.sign_in(&h, &alice, &did).await;
    let old = first["refresh_token"].as_str().unwrap();
    let said = app.refresh(&h, old).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["sub"], did.as_str());
    assert_eq!(said.body["scope"], SCOPE);
    let new = said.body["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(new, old);
    assert_eq!(app.refresh(&h, old).await.error(), (400, "invalid_grant"));
    // Only by its own client, with its own key.
    let other = App::loopback("atproto");
    assert_eq!(
        other.refresh(&h, &new).await.error(),
        (400, "invalid_grant")
    );
    let thief = App::new(&app.id);
    assert_eq!(
        thief.refresh(&h, &new).await.error(),
        (400, "invalid_dpop_key_binding")
    );
    let said = app
        .post(&h, "/oauth/token", &[("grant_type", "refresh_token")])
        .await;
    assert_eq!(said.error(), (400, "invalid_request"));
    // Not once it has lapsed.
    age(&h, "UPDATE oauth_token SET expires_at = 1");
    assert_eq!(app.refresh(&h, &new).await.error(), (400, "invalid_grant"));
}

#[tokio::test]
async fn ends_a_grant_by_either_token() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let app = App::loopback(SCOPE);
    for which in ["refresh_token", "access_token"] {
        let tokens = app.sign_in(&h, &alice, &did).await;
        let token = tokens[which].as_str().unwrap();
        let said = app.post(&h, "/oauth/revoke", &[("token", token)]).await;
        assert_eq!(said.status, 200, "{}", said.body);
        let refresh = tokens["refresh_token"].as_str().unwrap();
        assert_eq!(
            app.refresh(&h, refresh).await.error(),
            (400, "invalid_grant"),
            "{which}"
        );
    }
    // Whether there was one or not.
    assert_eq!(
        app.post(&h, "/oauth/revoke", &[("token", "never")])
            .await
            .status,
        200
    );
    assert_eq!(app.post(&h, "/oauth/revoke", &[]).await.status, 200);
    let res = h
        .http
        .post(format!("{}/oauth/revoke", h.url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn ends_a_members_grants_when_they_are_put_out() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    join(&h, &did, &bob.did, &["member"]).await;
    let app = App::loopback(SCOPE);
    let bobs = app.sign_in(&h, &bob, &did).await;
    let alices = app.sign_in(&h, &alice, &did).await;
    let lxm = format!("{NS}.ejectMember");
    let body = json!({"group": did, "member": bob.did});
    let ejected = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(ejected.0, 200, "{}", ejected.1);
    let refresh = |t: &Json| t["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(
        app.refresh(&h, &refresh(&bobs)).await.error(),
        (400, "invalid_grant")
    );
    assert_eq!(app.refresh(&h, &refresh(&alices)).await.status, 200);
}
