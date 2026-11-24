//! An app makes a group for someone and leaves with its own session on it.

mod common;

use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use serde_json::{Value as Json, json};

const PROVISION: &str = "org.atgrp.internal.provisionGroup";
const SCOPE: &str = "atproto repo:app.bsky.feed.post";

/// Asks `app` to provision as `who` with `body`, once more with the nonce
/// if asked.
async fn provision(h: &Running, app: &App, who: Option<&Authority>, body: &Json) -> Said {
    let send = || {
        let mut req = h
            .http
            .post(format!("{}/xrpc/{PROVISION}", h.url))
            .json(body)
            .header(
                "dpop",
                app.proof_with(h, "POST", &format!("/xrpc/{PROVISION}"), |_| {}),
            );
        if let Some(who) = who {
            let (k, v) = auth(h, who, PROVISION);
            req = req.header(k, v);
        }
        req
    };
    let said = app.said(send()).await;
    let stale = said
        .headers
        .get("www-authenticate")
        .is_some_and(|v| v.to_str().unwrap().contains("use_dpop_nonce"));
    if !stale {
        return said;
    }
    app.said(send()).await
}

fn asking(app: &App, extra: Json) -> Json {
    let mut body =
        json!({"handle": RIDERS, "displayName": "Riders", "scope": SCOPE, "clientId": app.id});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    body
}

#[tokio::test]
async fn makes_a_group_and_a_session_on_it_for_the_app() {
    let h = start(&[("PROVISIONING_CLIENTS", "*")], false).await;
    let [alice, ..] = people(&h).await;
    let app = App::loopback(SCOPE);
    let said = provision(
        &h,
        &app,
        Some(&alice),
        &asking(&app, json!({"joinPolicy": "invite"})),
    )
    .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let did = said.body["did"].as_str().unwrap().to_string();
    assert!(
        said.body["meta"]
            .as_str()
            .unwrap()
            .ends_with("/space/org.atgrp.meta/self")
    );
    assert!(
        said.body["members"]
            .as_str()
            .unwrap()
            .ends_with("/space/org.atgrp.members/self")
    );
    let session = &said.body["session"];
    assert_eq!(session["token_type"], "DPoP");
    assert_eq!(session["sub"], did.as_str());
    assert_eq!(session["scope"], SCOPE);
    // Alice founded it.
    let roles = h.host.roles(&did, &alice.did).unwrap();
    assert!(roles.contains(&"admin".to_string()), "{roles:?}");

    // The session writes as the group, refreshes, and is listed as Alice's.
    let token = session["access_token"].as_str().unwrap();
    let body = json!({"repo": did, "collection": "app.bsky.feed.post", "record": {"$type": "app.bsky.feed.post", "text": "hi", "createdAt": "2026-11-03T00:00:00Z"}});
    let made = app
        .call(&h, "com.atproto.repo.createRecord", token, Some(&body))
        .await;
    assert_eq!(made.status, 200, "{}", made.body);
    let refreshed = app
        .refresh(&h, session["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(refreshed.status, 200, "{}", refreshed.body);
    let lxm = format!("{INTERNAL}.listGroupTokens");
    let (_, grants) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    let sessions = grants["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["source"], "provisionGroup");
    assert_eq!(sessions[0]["member"], alice.did.as_str());
    assert_eq!(sessions[0]["clientId"], app.id.as_str());
}

#[tokio::test]
async fn refuses_before_it_makes_anything() {
    let h = start(
        &[("PROVISIONING_CLIENTS", "https://trusted.test/client.json")],
        false,
    )
    .await;
    let [alice, ..] = people(&h).await;
    let app = App::loopback(SCOPE);
    let ask = async |body: Json| provision(&h, &app, Some(&alice), &body).await;
    let refused = [
        (ask(asking(&app, json!({}))).await, (400, "UntrustedApp")),
        (
            ask(asking(&app, json!({"clientId": "nonsense"}))).await,
            (400, "invalid_client_metadata"),
        ),
        (
            ask(asking(
                &app,
                json!({"clientAssertion": "x", "clientAssertionType": "y"}),
            ))
            .await,
            (400, "invalid_client"),
        ),
        (
            ask(json!({"handle": RIDERS, "displayName": "Riders", "scope": SCOPE})).await,
            (400, "InvalidRequest"),
        ),
    ];
    for (said, error) in refused {
        assert_eq!(said.error(), error, "{}", said.body);
    }
    // Without service auth, or a proof.
    let said = provision(&h, &app, None, &asking(&app, json!({}))).await;
    assert_eq!(said.error(), (401, "AuthMissing"));
    let (k, v) = auth(&h, &alice, PROVISION);
    let bare = h
        .http
        .post(format!("{}/xrpc/{PROVISION}", h.url))
        .json(&asking(&app, json!({})))
        .header(k, v);
    assert_eq!(app.said(bare).await.error(), (401, "invalid_dpop_proof"));
    // Nothing was made.
    assert!(h.host.store.account(RIDERS).unwrap().is_none());
}

#[tokio::test]
async fn gives_the_app_what_it_declares_and_the_founder_may() {
    let h = start(&[("PROVISIONING_CLIENTS", "*")], false).await;
    let [alice, bob, _] = people(&h).await;
    let app = App::loopback(&format!("{SCOPE} rpc:org.atgrp.listModSubjects?aud=*"));
    for (scope, refused) in [
        ("repo:app.bsky.feed.post", "the scope must include atproto"),
        (
            "atproto repo:app.bsky.feed.like",
            "not in the app's client metadata: repo:app.bsky.feed.like",
        ),
    ] {
        let said = provision(
            &h,
            &app,
            Some(&alice),
            &asking(&app, json!({"scope": scope})),
        )
        .await;
        assert_eq!(said.error(), (400, "InvalidScope"));
        assert_eq!(said.body["message"], refused);
    }
    // A method of this host's is the founder's own to call, never the app's.
    let scope = format!("{SCOPE} rpc:org.atgrp.listModSubjects?aud=*");
    let said = provision(
        &h,
        &app,
        Some(&alice),
        &asking(&app, json!({"scope": scope})),
    )
    .await;
    assert_eq!(said.body["session"]["scope"], SCOPE);
    // A handle taken is refused as createGroup refuses it.
    let said = provision(&h, &app, Some(&bob), &asking(&app, json!({}))).await;
    assert_eq!(said.status, 400, "{}", said.body);
}
