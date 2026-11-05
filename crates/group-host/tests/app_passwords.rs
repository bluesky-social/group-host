//! App passwords: made by whoever may do anything as the group, signed in
//! with by apps that take a handle and a password, and ending the moment
//! their maker could no longer make them.

mod common;

use atproto::jose;
use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::members_space;
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const POST: &str = "app.bsky.feed.post";

async fn put_membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let collection = format!("{NS}.membership");
    let value = json!({"$type": collection, "member": who, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let rkey = who.to_string();
    let exists = h
        .host
        .group_record(did, &members_space(did), &collection, &rkey)
        .unwrap()
        .is_some();
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
    h.host
        .space_write(did, &members_space(did), &[write])
        .await
        .unwrap();
}

/// Riders, Alice's; Bob a publisher, who may write every collection as the
/// group but not configure it; Carol a plain member.
async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let collection = format!("{NS}.permissions");
    let mut permissions = h
        .host
        .group_record(&did, &members_space(&did), &collection, "self")
        .unwrap()
        .unwrap();
    let publisher = json!({"role": "publisher", "scopes": [{"scope": "repo:*"}], "assignable": []});
    permissions["grants"]
        .as_array_mut()
        .unwrap()
        .push(publisher);
    let write = Write::Update {
        collection,
        rkey: "self".into(),
        value: permissions,
    };
    h.host
        .space_write(&did, &members_space(&did), &[write])
        .await
        .unwrap();
    put_membership(h, &did, &bob.did, &["member", "publisher"]).await;
    put_membership(h, &did, &carol.did, &["member"]).await;
    (did, [alice, bob, carol])
}

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn make(h: &Running, who: &Authority, group: &str, name: &str) -> String {
    let (status, made) = call(
        h,
        who,
        "createAppPassword",
        json!({"group": group, "name": name}),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    made["password"].as_str().unwrap().to_string()
}

async fn xrpc(h: &Running, nsid: &str, bearer: Option<&str>, body: Option<Json>) -> (u16, Json) {
    let url = format!("{}/xrpc/{nsid}", h.url);
    let mut req = match &body {
        Some(b) => h.http.post(url).json(b),
        None => h.http.get(url),
    };
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let res = req.send().await.unwrap();
    (
        res.status().as_u16(),
        res.json().await.unwrap_or(Json::Null),
    )
}

async fn sign_in(h: &Running, identifier: &str, password: &str) -> (u16, Json) {
    let body = json!({"identifier": identifier, "password": password});
    xrpc(h, "com.atproto.server.createSession", None, Some(body)).await
}

async fn list(h: &Running, who: &Authority, group: &str) -> (u16, Json) {
    let lxm = format!("{NS}.listAppPasswords");
    get(h, &lxm, &format!("group={group}"), &[auth(h, who, &lxm)]).await
}

fn names(listed: &Json) -> Vec<&str> {
    listed["passwords"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect()
}

fn record(text: &str) -> Json {
    json!({"$type": POST, "text": text, "createdAt": "2026-11-03T00:00:00Z"})
}

#[tokio::test]
async fn is_made_by_whoever_may_do_anything_as_the_group() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let (status, made) = call(
        &h,
        &alice,
        "createAppPassword",
        json!({"group": did, "name": " Phone ", "privileged": true}),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    assert_eq!(made["name"], "Phone");
    assert_eq!(made["privileged"], true);
    let password = made["password"].as_str().unwrap();
    assert_eq!(password.len(), 19);
    assert!(
        password.split('-').all(|p| p.len() == 4
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))),
        "{password}"
    );
    make(&h, &bob, &did, "Laptop").await;
    // A privileged one's sessions may use chat too.
    assert_eq!(sign_in(&h, RIDERS, password).await.0, 200);
    let lxm = format!("{NS}.listGroupTokens");
    let (_, grants) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(
        grants["sessions"][0]["scope"],
        "atproto transition:generic transition:chat.bsky"
    );

    for (who, body, refused) in [
        (
            &carol,
            json!({"group": did, "name": "Mine"}),
            (403, "Forbidden"),
        ),
        (
            &alice,
            json!({"group": did, "name": "  "}),
            (400, "InvalidName"),
        ),
        (&alice, json!({"group": did}), (400, "InvalidRequest")),
        (
            &alice,
            json!({"group": did, "name": "x".repeat(65)}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "name": "Phone"}),
            (400, "DuplicateName"),
        ),
        (
            &alice,
            json!({"group": "did:plc:nothing", "name": "Phone"}),
            (400, "GroupNotFound"),
        ),
        (&alice, json!({"name": "Phone"}), (400, "InvalidRequest")),
    ] {
        let said = call(&h, who, "createAppPassword", body.clone()).await;
        assert_eq!(
            (said.0, said.1["error"].as_str().unwrap_or_default()),
            refused,
            "{body}"
        );
    }
    for i in 0..23 {
        make(&h, &alice, &did, &format!("Device {i}")).await;
    }
    let said = call(
        &h,
        &alice,
        "createAppPassword",
        json!({"group": did, "name": "One more"}),
    )
    .await;
    assert_eq!(said.1["error"], "TooManyAppPasswords");

    // Its configurers see every one; its maker their own; others none.
    let (_, all) = list(&h, &alice, &did).await;
    assert_eq!(all["passwords"].as_array().unwrap().len(), 25);
    assert!(
        all["passwords"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("password").is_none())
    );
    let (_, own) = list(&h, &bob, &did).await;
    assert_eq!(names(&own), ["Laptop"]);
    assert_eq!(own["passwords"][0]["member"], bob.did.as_str());
    assert_eq!(list(&h, &carol, &did).await.0, 403);
    assert_eq!(list(&h, &alice, "did:plc:nothing").await.0, 400);
}

#[tokio::test]
async fn signs_an_app_in_to_the_groups_repo() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let password = make(&h, &alice, &did, "Phone").await;
    for identifier in [
        RIDERS.to_string(),
        format!(" @{} ", RIDERS.to_uppercase()),
        did.clone(),
    ] {
        let (status, session) = sign_in(&h, &identifier, &password.to_uppercase()).await;
        assert_eq!(status, 200, "{identifier}: {session}");
        assert_eq!(session["did"], did.as_str());
        assert_eq!(session["handle"], RIDERS);
        assert_eq!(session["active"], true);
        assert_eq!(session["emailConfirmed"], false);
        assert_eq!(session["didDoc"]["id"], did.as_str());
    }
    let (_, session) = sign_in(&h, RIDERS, &password).await;
    let access = session["accessJwt"].as_str().unwrap();
    // A JWT an app may read the lapse of, as it reads a PDS's.
    let (header, claims) = jose::parts(access).unwrap();
    assert_eq!(header["alg"], "HS256");
    assert_eq!(claims["sub"], did.as_str());
    assert!(claims["exp"].as_i64().unwrap() > claims["iat"].as_i64().unwrap());

    for (identifier, password) in [
        (RIDERS, "aaaa-bbbb-cccc-dddd"),
        ("nobody.test", password.as_str()),
        (alice.did.as_str(), password.as_str()),
    ] {
        let (status, said) = sign_in(&h, identifier, password).await;
        assert_eq!(
            (status, said["error"].clone()),
            (401, json!("AuthenticationRequired")),
            "{identifier}"
        );
    }
    let (status, _) = xrpc(
        &h,
        "com.atproto.server.createSession",
        None,
        Some(json!({"identifier": RIDERS})),
    )
    .await;
    assert_eq!(status, 400);
    let (status, _) = xrpc(
        &h,
        "com.atproto.server.createSession",
        None,
        Some(json!({"password": "x"})),
    )
    .await;
    assert_eq!(status, 400);

    // The group's repo, any collection, and blobs; never a space.
    let made = xrpc(
        &h,
        "com.atproto.repo.createRecord",
        Some(access),
        Some(json!({"repo": did, "collection": POST, "record": record("hi")})),
    )
    .await;
    assert_eq!(made.0, 200, "{}", made.1);
    let space = format!("at://{did}/space/fyi.opensocial.meta/self");
    let rule = json!({"repo": did, "space": space, "collection": format!("{NS}.rule"), "record": {"$type": format!("{NS}.rule"), "title": "x", "text": "x"}});
    let said = xrpc(
        &h,
        "com.atproto.space.createRecord",
        Some(access),
        Some(rule),
    )
    .await;
    assert_eq!(said.1["error"], "ScopeMissing");
    let said = xrpc(&h, "com.atproto.server.getSession", Some(access), None).await;
    assert_eq!(said.1["did"], did.as_str());
    let (_, listed) = list(&h, &alice, &did).await;
    assert!(listed["passwords"][0]["lastUsedAt"].is_string(), "{listed}");
    let lxm = format!("{NS}.listGroupTokens");
    let (_, grants) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    let sessions = grants["sessions"].as_array().unwrap();
    assert!(
        sessions
            .iter()
            .all(|s| s["source"] == "appPassword" && s["clientId"] == "app-password:Phone")
    );
    assert_eq!(sessions[0]["scope"], "atproto transition:generic");

    // A token of another kind, or of nothing, is no session.
    let app = App::loopback("atproto");
    let oauth = app.sign_in(&h, &alice, &did).await;
    for token in [oauth["access_token"].as_str().unwrap(), "nothing"] {
        let said = xrpc(&h, "com.atproto.server.getSession", Some(token), None).await;
        assert_eq!(
            (said.0, said.1["error"].clone()),
            (401, json!("InvalidToken"))
        );
    }
    // A lapsed one says so, for the app to refresh.
    age(
        &h,
        "UPDATE oauth_token SET access_expires = 1 WHERE source = 'appPassword'",
    );
    let said = xrpc(&h, "com.atproto.server.getSession", Some(access), None).await;
    assert_eq!(said.1["error"], "ExpiredToken");
}

#[tokio::test]
async fn refreshes_and_signs_out() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let password = make(&h, &alice, &did, "Phone").await;
    let (_, session) = sign_in(&h, RIDERS, &password).await;
    let refresh = session["refreshJwt"].as_str().unwrap();
    let (status, fresh) = xrpc(
        &h,
        "com.atproto.server.refreshSession",
        Some(refresh),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 200, "{fresh}");
    assert_ne!(fresh["refreshJwt"], session["refreshJwt"]);
    assert_eq!(fresh["did"], did.as_str());
    let old = xrpc(
        &h,
        "com.atproto.server.refreshSession",
        Some(refresh),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        (old.0, old.1["error"].clone()),
        (401, json!("InvalidToken"))
    );
    let said = xrpc(
        &h,
        "com.atproto.server.getSession",
        Some(session["accessJwt"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(said.0, 401);
    let none = xrpc(
        &h,
        "com.atproto.server.refreshSession",
        None,
        Some(json!({})),
    )
    .await;
    assert_eq!(none.1["error"], "AuthMissing");

    let access = fresh["accessJwt"].as_str().unwrap();
    let refresh = fresh["refreshJwt"].as_str().unwrap();
    let out = xrpc(
        &h,
        "com.atproto.server.deleteSession",
        Some(refresh),
        Some(json!({})),
    )
    .await;
    assert_eq!(out.0, 200, "{}", out.1);
    assert_eq!(
        xrpc(&h, "com.atproto.server.getSession", Some(access), None)
            .await
            .0,
        401
    );

    // A switched-off group's session says so.
    let (_, session) = sign_in(&h, RIDERS, &password).await;
    call(&h, &alice, "deactivateGroup", json!({"group": did})).await;
    let refresh = session["refreshJwt"].as_str().unwrap();
    let (_, fresh) = xrpc(
        &h,
        "com.atproto.server.refreshSession",
        Some(refresh),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        (fresh["active"].clone(), fresh["status"].clone()),
        (json!(false), json!("deactivated"))
    );
}

#[tokio::test]
async fn ends_once_its_maker_could_no_longer_make_it() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let password = make(&h, &bob, &did, "Laptop").await;
    let (_, session) = sign_in(&h, RIDERS, &password).await;
    let (_, second) = sign_in(&h, RIDERS, &password).await;
    let access = session["accessJwt"].as_str().unwrap();
    let write = json!({"repo": did, "collection": POST, "record": record("hi")});
    assert_eq!(
        xrpc(
            &h,
            "com.atproto.repo.createRecord",
            Some(access),
            Some(write.clone())
        )
        .await
        .0,
        200
    );
    // Bob is a publisher no more.
    put_membership(&h, &did, &bob.did, &["member"]).await;
    // Its refresh is refused, and that ends every session of his.
    let refresh = second["refreshJwt"].as_str().unwrap();
    assert_eq!(
        xrpc(
            &h,
            "com.atproto.server.refreshSession",
            Some(refresh),
            Some(json!({}))
        )
        .await
        .0,
        401
    );
    assert_eq!(sign_in(&h, RIDERS, &password).await.0, 401);
    let said = xrpc(
        &h,
        "com.atproto.repo.createRecord",
        Some(access),
        Some(write),
    )
    .await;
    assert_eq!(
        (said.0, said.1["error"].clone()),
        (401, json!("InvalidToken"))
    );
    let (_, all) = list(&h, &alice, &did).await;
    assert_eq!(all["passwords"], json!([]));
    // Or its next use is, likewise.
    put_membership(&h, &did, &bob.did, &["member", "publisher"]).await;
    let password = make(&h, &bob, &did, "Laptop").await;
    let (_, session) = sign_in(&h, RIDERS, &password).await;
    put_membership(&h, &did, &bob.did, &["member"]).await;
    let access = session["accessJwt"].as_str().unwrap();
    let said = xrpc(&h, "com.atproto.server.getSession", Some(access), None).await;
    assert_eq!(
        (said.0, said.1["error"].clone()),
        (401, json!("InvalidToken"))
    );
    assert_eq!(list(&h, &alice, &did).await.1["passwords"], json!([]));

    // And when they leave.
    put_membership(&h, &did, &carol.did, &["member", "publisher"]).await;
    let password = make(&h, &carol, &did, "Tablet").await;
    let (_, session) = sign_in(&h, RIDERS, &password).await;
    let (status, ejected) = call(&h, &carol, "leaveGroup", json!({"group": did})).await;
    assert_eq!(status, 200, "{ejected}");
    let said = xrpc(
        &h,
        "com.atproto.server.getSession",
        Some(session["accessJwt"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(said.0, 401);
    assert_eq!(names(&list(&h, &alice, &did).await.1), Vec::<&str>::new());
}

#[tokio::test]
async fn is_forgotten_once_its_maker_could_no_longer_make_it_though_never_used() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    // Laptop is listed no more once Bob is a publisher no more, and does
    // not come back with his role.
    let laptop = make(&h, &bob, &did, "Laptop").await;
    put_membership(&h, &did, &bob.did, &["member"]).await;
    assert_eq!(names(&list(&h, &alice, &did).await.1), Vec::<&str>::new());
    put_membership(&h, &did, &bob.did, &["member", "publisher"]).await;
    assert_eq!(sign_in(&h, RIDERS, &laptop).await.0, 401);
    // Nor does Tablet, refused while he was not one.
    let tablet = make(&h, &bob, &did, "Tablet").await;
    put_membership(&h, &did, &bob.did, &["member"]).await;
    assert_eq!(sign_in(&h, RIDERS, &tablet).await.0, 401);
    put_membership(&h, &did, &bob.did, &["member", "publisher"]).await;
    assert_eq!(sign_in(&h, RIDERS, &tablet).await.0, 401);
    assert_eq!(names(&list(&h, &bob, &did).await.1), Vec::<&str>::new());
}

#[tokio::test]
async fn is_revoked_by_its_maker_or_a_configurer() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let bobs = make(&h, &bob, &did, "Laptop").await;
    make(&h, &alice, &did, "Phone").await;
    let (_, session) = sign_in(&h, RIDERS, &bobs).await;
    let revoke = async |who: &Authority, body: Json| call(&h, who, "revokeAppPassword", body).await;
    assert_eq!(
        revoke(&bob, json!({"group": did, "name": "Phone"})).await.0,
        403
    );
    assert_eq!(
        revoke(&bob, json!({"group": did, "name": "Tablet"}))
            .await
            .1["error"],
        "NotFound"
    );
    assert_eq!(revoke(&bob, json!({"group": did})).await.0, 400);
    assert_eq!(revoke(&bob, json!({"name": "Laptop"})).await.0, 400);
    assert_eq!(
        revoke(&bob, json!({"group": "did:plc:nothing", "name": "Laptop"}))
            .await
            .0,
        400
    );
    let (status, done) = revoke(&bob, json!({"group": did, "name": "Laptop"})).await;
    assert_eq!(status, 200, "{done}");
    let said = xrpc(
        &h,
        "com.atproto.server.getSession",
        Some(session["accessJwt"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(said.0, 401);
    assert_eq!(sign_in(&h, RIDERS, &bobs).await.0, 401);
    assert_eq!(
        revoke(&alice, json!({"group": did, "name": "Phone"}))
            .await
            .0,
        200
    );
    assert_eq!(names(&list(&h, &alice, &did).await.1), Vec::<&str>::new());
    let lxm = format!("{NS}.listGroupTokens");
    let (_, grants) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    let actions: Vec<&str> = grants["activity"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions[0], "revoked the app password \"Phone\"");
    assert!(
        actions.contains(&"signed in with the app password \"Laptop\""),
        "{actions:?}"
    );
}
