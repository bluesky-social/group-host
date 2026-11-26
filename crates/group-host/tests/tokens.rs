//! Using a token: the resource server's checks and challenges, and writing
//! as the group in its repo and spaces, as far as the token's scope and
//! its member's roles reach.

mod common;

use atproto::space_auth::{self, Kind, sign_request};
use atproto::{Curve, Keypair};
use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start, start_reachable};
use group_host::group::{members_space, meta_space};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const POST: &str = "app.bsky.feed.post";
const CALENDAR: &str = "com.example.calendar";
const EVENT: &str = "com.example.event";

async fn membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let value = json!({"$type": format!("{NS}.membership"), "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let rkey = who.to_string();
    let exists = h
        .host
        .group_record(did, &members_space(did), &format!("{NS}.membership"), &rkey)
        .unwrap()
        .is_some();
    let collection = format!("{NS}.membership");
    let write = if exists {
        Write::Update {
            collection,
            rkey,
            value,
        }
    } else {
        Write::Create {
            collection,
            rkey,
            value,
        }
    };
    h.host
        .space_write(did, &members_space(did), &[write])
        .await
        .unwrap();
}

/// Riders, Alice's; a calendar its members read, where members add events
/// as the group and admins write anything; Bob a member.
async fn group(h: &Running) -> (String, String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let policy = json!({"$type": "org.atgrp.policy", "grants": [
        {"role": "member", "read": true, "write": true, "scopes": [format!("space:{CALENDAR}?collection={EVENT}&action=create")]},
        {"role": "admin", "read": true, "write": true, "scopes": [format!("space:{CALENDAR}?collection=*")]},
    ]});
    let made = make_space(h, &alice, &did, CALENDAR, "rides", policy).await;
    membership(h, &did, &bob.did, &["member"]).await;
    (did, made, [alice, bob, carol])
}

/// A token for `app`, `who` approving it for `group`.
async fn token(h: &Running, app: &App, who: &Authority, group: &str) -> String {
    let tokens = app.sign_in(h, who, group).await;
    tokens["access_token"].as_str().unwrap().to_string()
}

fn record(collection: &str, text: &str) -> Json {
    json!({"$type": collection, "text": text, "createdAt": "2026-11-03T00:00:00Z"})
}

#[tokio::test]
async fn challenges_a_request_without_a_good_token() {
    let h = start(&[], false).await;
    let (did, _, [alice, ..]) = group(&h).await;
    let app = App::loopback(&format!("atproto repo:{POST}"));
    let tokens = app.sign_in(&h, &alice, &did).await;
    let access = tokens["access_token"].as_str().unwrap();
    let nsid = "com.atproto.server.getSession";
    let url = format!("{}/xrpc/{nsid}", h.url);

    let said = app.said(h.http.get(&url)).await;
    assert_eq!(said.error(), (401, "AuthMissing"));
    let said = app
        .said(
            h.http
                .get(&url)
                .header("authorization", format!("Bearer {access}")),
        )
        .await;
    // As a bearer token it is no app password's session.
    assert_eq!(said.error(), (401, "InvalidToken"));
    let said = app
        .said(
            h.http
                .get(&url)
                .header("authorization", format!("Basic {access}")),
        )
        .await;
    assert_eq!(said.error(), (401, "invalid_token"));
    let challenge = said.headers["www-authenticate"].to_str().unwrap();
    assert!(
        challenge.starts_with(r#"DPoP error="invalid_token""#),
        "{challenge}"
    );
    assert!(said.headers.contains_key("dpop-nonce"));

    let bare = |token: &str| {
        h.http
            .get(&url)
            .header("authorization", format!("DPoP {token}"))
    };
    let said = app.said(bare(access)).await;
    assert_eq!(said.error(), (401, "invalid_dpop_proof"));
    let said = app
        .said(bare("never").header("dpop", app.resource_proof(&h, "GET", nsid, "never")))
        .await;
    assert_eq!(said.error(), (401, "invalid_token"));
    // A proof of another token, or by another key.
    let said = app
        .said(bare(access).header("dpop", app.resource_proof(&h, "GET", nsid, "other")))
        .await;
    assert!(
        said.body["message"].as_str().unwrap().contains("ath"),
        "{}",
        said.body
    );
    let thief = App::new(&app.id);
    *thief.nonce.lock().unwrap() = app.nonce.lock().unwrap().clone();
    let said = thief
        .said(bare(access).header("dpop", thief.resource_proof(&h, "GET", nsid, access)))
        .await;
    assert_eq!(said.body["message"], "the DPoP key is not the token's");
    // A stale nonce is asked for again.
    *app.nonce.lock().unwrap() = Some("stale".into());
    let said = app.call(&h, nsid, access, None).await;
    assert_eq!(said.status, 200, "{}", said.body);

    // A refreshed token's old access token, and a lapsed one, are no more.
    let refreshed = app
        .refresh(&h, tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(
        app.call(&h, nsid, access, None).await.error(),
        (401, "invalid_token")
    );
    let fresh = refreshed.body["access_token"].as_str().unwrap();
    assert_eq!(app.call(&h, nsid, fresh, None).await.status, 200);
    age(&h, "UPDATE oauth_token SET access_expires = 1");
    assert_eq!(
        app.call(&h, nsid, fresh, None).await.error(),
        (401, "invalid_token")
    );
}

#[tokio::test]
async fn says_whom_a_token_is_for() {
    let h = start(&[], false).await;
    let (did, _, [alice, ..]) = group(&h).await;
    let app = App::loopback("atproto");
    let access = token(&h, &app, &alice, &did).await;
    let said = app
        .call(&h, "com.atproto.server.getSession", &access, None)
        .await;
    assert_eq!(
        said.body,
        json!({"did": did, "handle": RIDERS, "active": true})
    );
    let lxm = format!("{INTERNAL}.deactivateGroup");
    post(&h, &lxm, &json!({"group": did}), &[auth(&h, &alice, &lxm)]).await;
    let said = app
        .call(&h, "com.atproto.server.getSession", &access, None)
        .await;
    assert_eq!(said.body["active"], false);
    assert_eq!(said.body["status"], "deactivated");
    // And writes nothing while it is switched off.
    let app = App::loopback(&format!("atproto repo:{POST}"));
    let said = app
        .call(
            &h,
            "com.atproto.repo.createRecord",
            &access,
            Some(&json!({"repo": did, "collection": POST, "record": record(POST, "hi")})),
        )
        .await;
    assert_eq!(said.status, 401);
}

#[tokio::test]
async fn writes_the_groups_repo_within_its_scope() {
    let h = start(&[("BLOB_UPLOAD_LIMIT", "3000000")], false).await;
    let (did, _, [alice, ..]) = group(&h).await;
    let app = App::loopback(&format!("atproto repo:{POST} blob:image/*"));
    let access = token(&h, &app, &alice, &did).await;
    let call = async |nsid: &str, body: Json| {
        app.call(
            &h,
            &format!("com.atproto.repo.{nsid}"),
            &access,
            Some(&body),
        )
        .await
    };

    let made = call(
        "createRecord",
        json!({"repo": did, "collection": POST, "record": record(POST, "hello")}),
    )
    .await;
    assert_eq!(made.status, 200, "{}", made.body);
    let uri = made.body["uri"].as_str().unwrap();
    assert!(uri.starts_with(&format!("at://{did}/{POST}/")), "{uri}");
    assert!(made.body["commit"]["rev"].is_string());
    let (_, read) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!(
                "repo={did}&collection={POST}&rkey={}",
                uri.rsplit('/').next().unwrap()
            ),
        )
        .await;
    assert_eq!(read["value"]["text"], "hello");

    let named = call(
        "createRecord",
        json!({"repo": RIDERS, "collection": POST, "rkey": "first", "record": record(POST, "one")}),
    )
    .await;
    assert_eq!(named.body["uri"], format!("at://{did}/{POST}/first"));
    // A put of what is there makes no commit; a change does.
    let same = call(
        "putRecord",
        json!({"repo": did, "collection": POST, "rkey": "first", "record": record(POST, "one")}),
    )
    .await;
    assert_eq!(same.status, 200, "{}", same.body);
    assert_eq!(same.body.get("commit"), None);
    let changed = call(
        "putRecord",
        json!({"repo": did, "collection": POST, "rkey": "first", "record": record(POST, "two")}),
    )
    .await;
    assert!(
        changed.body["commit"]["cid"].is_string(),
        "{}",
        changed.body
    );
    let put_new = call(
        "putRecord",
        json!({"repo": did, "collection": POST, "rkey": "second", "record": record(POST, "new")}),
    )
    .await;
    assert!(put_new.body["commit"].is_object(), "{}", put_new.body);

    let gone = call(
        "deleteRecord",
        json!({"repo": did, "collection": POST, "rkey": "first"}),
    )
    .await;
    assert!(gone.body["commit"].is_object(), "{}", gone.body);
    let again = call(
        "deleteRecord",
        json!({"repo": did, "collection": POST, "rkey": "first"}),
    )
    .await;
    assert_eq!((again.status, again.body.clone()), (200, json!({})));

    let applied = call("applyWrites", json!({"repo": did, "writes": [
        {"$type": "com.atproto.repo.applyWrites#create", "collection": POST, "rkey": "a", "value": record(POST, "a")},
        {"$type": "com.atproto.repo.applyWrites#create", "collection": POST, "value": record(POST, "b")},
        {"$type": "com.atproto.repo.applyWrites#update", "collection": POST, "rkey": "second", "value": record(POST, "c")},
        {"$type": "com.atproto.repo.applyWrites#delete", "collection": POST, "rkey": "a"},
    ]})).await;
    assert_eq!(applied.status, 200, "{}", applied.body);
    let kinds: Vec<&str> = applied.body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["$type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "com.atproto.repo.applyWrites#createResult",
            "com.atproto.repo.applyWrites#createResult",
            "com.atproto.repo.applyWrites#updateResult",
            "com.atproto.repo.applyWrites#deleteResult",
        ]
    );
    assert_eq!(
        applied.body["results"][0]["uri"],
        format!("at://{did}/{POST}/a")
    );

    // Nothing outside its scope, nor in another repo, nor half-said.
    let like = "app.bsky.feed.like";
    for (nsid, body, refused) in [
        (
            "createRecord",
            json!({"repo": did, "collection": like, "record": record(like, "x")}),
            (403, "ScopeMissing"),
        ),
        (
            "deleteRecord",
            json!({"repo": did, "collection": like, "rkey": "x"}),
            (403, "ScopeMissing"),
        ),
        (
            "createRecord",
            json!({"repo": alice.did, "collection": POST, "record": record(POST, "x")}),
            (403, "Forbidden"),
        ),
        (
            "createRecord",
            json!({"collection": POST, "record": record(POST, "x")}),
            (400, "InvalidRequest"),
        ),
        (
            "putRecord",
            json!({"repo": did, "collection": POST, "rkey": "x", "record": {"$type": like}}),
            (400, "InvalidRequest"),
        ),
        ("applyWrites", json!({"repo": did}), (400, "InvalidRequest")),
        (
            "applyWrites",
            json!({"repo": did, "writes": [{"$type": "com.atproto.repo.applyWrites#move", "collection": POST}]}),
            (400, "InvalidRequest"),
        ),
        (
            "applyWrites",
            json!({"repo": did, "writes": [{"$type": "com.atproto.repo.applyWrites#update", "collection": POST, "value": {}}]}),
            (400, "InvalidRequest"),
        ),
        (
            "applyWrites",
            json!({"repo": did, "writes": [{"$type": "com.atproto.repo.applyWrites#delete", "collection": POST}]}),
            (400, "InvalidRequest"),
        ),
        (
            "applyWrites",
            json!({"repo": did, "writes": [{"$type": "com.atproto.repo.applyWrites#delete", "rkey": "x"}]}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = call(nsid, body.clone()).await;
        assert_eq!(said.error(), refused, "{nsid} {body}: {}", said.body);
    }
    let many: Vec<Json> = (0..201).map(|i| json!({"$type": "com.atproto.repo.applyWrites#create", "collection": POST, "rkey": format!("r{i}"), "value": record(POST, "x")})).collect();
    assert_eq!(
        call("applyWrites", json!({"repo": did, "writes": many}))
            .await
            .error(),
        (400, "InvalidRequest")
    );

    // A write that swaps must find what it says it swaps.
    let head = h.host.store.head(&did).unwrap().unwrap().cid.to_string();
    let second = h
        .host
        .store
        .record(&did, POST, "second")
        .unwrap()
        .unwrap()
        .cid
        .to_string();
    for (nsid, body, status) in [
        (
            "createRecord",
            json!({"repo": did, "collection": POST, "record": record(POST, "x"), "swapCommit": "bafyother"}),
            400,
        ),
        (
            "putRecord",
            json!({"repo": did, "collection": POST, "rkey": "second", "record": record(POST, "x"), "swapRecord": null}),
            400,
        ),
        (
            "putRecord",
            json!({"repo": did, "collection": POST, "rkey": "third", "record": record(POST, "x"), "swapRecord": second}),
            400,
        ),
        (
            "deleteRecord",
            json!({"repo": did, "collection": POST, "rkey": "second", "swapRecord": "bafyother"}),
            400,
        ),
        (
            "applyWrites",
            json!({"repo": did, "swapCommit": "bafyother", "writes": []}),
            400,
        ),
        (
            "putRecord",
            json!({"repo": did, "collection": POST, "rkey": "third", "record": record(POST, "x"), "swapRecord": null, "swapCommit": head}),
            200,
        ),
        (
            "deleteRecord",
            json!({"repo": did, "collection": POST, "rkey": "second", "swapRecord": second}),
            200,
        ),
    ] {
        let said = call(nsid, body.clone()).await;
        assert_eq!(said.status, status, "{body}: {}", said.body);
        if status == 400 {
            assert_eq!(said.body["error"], "InvalidSwap");
        }
    }

    // A blob, of a type its scope takes.
    let upload = async |mime: &str, bytes: Vec<u8>| {
        let nsid = "com.atproto.repo.uploadBlob";
        let req = h
            .http
            .post(format!("{}/xrpc/{nsid}", h.url))
            .header("authorization", format!("DPoP {access}"))
            .header("dpop", app.resource_proof(&h, "POST", nsid, &access))
            .header("content-type", mime)
            .body(bytes);
        app.said(req).await
    };
    let said = upload("image/png", b"face".to_vec()).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["blob"]["mimeType"], "image/png");
    assert_eq!(
        upload("video/mp4", b"face".to_vec()).await.error(),
        (403, "ScopeMissing")
    );
    // Up to the host's limit, past the 2 MB a body is held to by default;
    // past it, refused as the lexicon names it.
    let said = upload("image/png", vec![7; 2_500_000]).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["blob"]["size"], 2_500_000);
    let said = upload("image/png", vec![7; 3_500_000]).await;
    assert_eq!(said.error(), (413, "BlobTooLarge"));
    // A caller without a token is turned away before its body is read.
    let res = h
        .http
        .post(format!("{}/xrpc/com.atproto.repo.uploadBlob", h.url))
        .header("content-type", "image/png")
        .body(vec![7; 3_500_000])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn writes_only_what_its_member_still_may() {
    let h = start(&[], false).await;
    let (did, calendar, [_, bob, _]) = group(&h).await;
    membership(&h, &did, &bob.did, &["member", "admin"]).await;
    let scope = format!(
        "atproto repo:{POST} include:org.atgrp.authManageGroup space:{CALENDAR}?collection=*"
    );
    let app = App::loopback(&scope);
    let access = token(&h, &app, &bob, &did).await;
    let post_one = async || {
        app.call(
            &h,
            "com.atproto.repo.createRecord",
            &access,
            Some(&json!({"repo": did, "collection": POST, "record": record(POST, "x")})),
        )
        .await
    };
    let rule = async || {
        app.call(&h, "com.atproto.space.createRecord", &access, Some(&json!({"repo": did, "space": meta_space(&did), "collection": format!("{NS}.rule"), "record": {"$type": format!("{NS}.rule"), "title": "Be kind", "text": "Be kind.", "createdAt": "2026-11-03T00:00:00Z"}}))).await
    };
    let event = async || {
        app.call(&h, "com.atproto.space.createRecord", &access, Some(&json!({"repo": did, "space": calendar, "collection": "com.example.note", "record": record("com.example.note", "x")}))).await
    };
    assert_eq!(post_one().await.status, 200);
    assert_eq!(rule().await.status, 200);
    assert_eq!(event().await.status, 200);
    // Bob is an admin no more: the app gets less at once.
    membership(&h, &did, &bob.did, &["member"]).await;
    for said in [post_one().await, rule().await, event().await] {
        assert_eq!(said.status, 403, "{}", said.body);
        assert!(
            said.body["message"]
                .as_str()
                .unwrap()
                .contains("may no longer write"),
            "{}",
            said.body
        );
    }
}

#[tokio::test]
async fn uploads_only_while_its_member_may_write_and_the_group_is_on() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    membership(&h, &did, &bob.did, &["member", "admin"]).await;
    let app = App::loopback(&format!("atproto repo:{POST} blob:image/*"));
    let access = token(&h, &app, &bob, &did).await;
    let upload = async || {
        let nsid = "com.atproto.repo.uploadBlob";
        let req = h
            .http
            .post(format!("{}/xrpc/{nsid}", h.url))
            .header("authorization", format!("DPoP {access}"))
            .header("dpop", app.resource_proof(&h, "POST", nsid, &access))
            .header("content-type", "image/png")
            .body(b"face".to_vec());
        app.said(req).await
    };
    assert_eq!(upload().await.status, 200);
    // Not while the group is switched off.
    let off = format!("{INTERNAL}.deactivateGroup");
    post(&h, &off, &json!({"group": did}), &[auth(&h, &alice, &off)]).await;
    assert_eq!(upload().await.error(), (400, "RepoDeactivated"));
    let on = format!("{INTERNAL}.activateGroup");
    post(&h, &on, &json!({"group": did}), &[auth(&h, &alice, &on)]).await;
    assert_eq!(upload().await.status, 200);
    // While Bob, an admin no more, may still add events in the calendar,
    // the app may upload; once he may write nothing as the group, not.
    membership(&h, &did, &bob.did, &["member"]).await;
    assert_eq!(upload().await.status, 200);
    let readers =
        json!({"$type": format!("{NS}.policy"), "grants": [{"role": "member", "read": true}]});
    h.host.store.set_policy(&calendar, &readers).unwrap();
    let said = upload().await;
    assert_eq!(said.status, 403, "{}", said.body);
}

#[tokio::test]
async fn writes_the_groups_spaces_within_its_scope() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    let scope = format!("atproto space:{CALENDAR}?collection={EVENT}&action=create");
    let app = App::loopback(&scope);
    let access = token(&h, &app, &bob, &did).await;
    let call = async |nsid: &str, body: Json| {
        app.call(
            &h,
            &format!("com.atproto.space.{nsid}"),
            &access,
            Some(&body),
        )
        .await
    };
    let made = call("createRecord", json!({"repo": did, "space": calendar, "collection": EVENT, "record": record(EVENT, "ride")})).await;
    assert_eq!(made.status, 200, "{}", made.body);
    let uri = made.body["uri"].as_str().unwrap();
    assert!(
        uri.starts_with(&format!("{calendar}/{did}/{EVENT}/")),
        "{uri}"
    );
    let rkey = uri.rsplit('/').next().unwrap();
    let stored = h
        .host
        .store
        .space_record(&did, &calendar, EVENT, rkey)
        .unwrap();
    assert!(stored.is_some());
    let applied = call("applyWrites", json!({"repo": did, "space": calendar, "writes": [
        {"$type": "com.atproto.space.applyWrites#create", "collection": EVENT, "rkey": "b", "value": record(EVENT, "b")},
    ]})).await;
    assert_eq!(
        applied.body["results"][0]["$type"],
        "com.atproto.space.applyWrites#createResult"
    );
    // Its scope, and Bob's roles, create events and nothing more.
    for (nsid, body) in [
        (
            "putRecord",
            json!({"repo": did, "space": calendar, "collection": EVENT, "rkey": rkey, "record": record(EVENT, "changed")}),
        ),
        (
            "deleteRecord",
            json!({"repo": did, "space": calendar, "collection": EVENT, "rkey": rkey}),
        ),
        (
            "createRecord",
            json!({"repo": did, "space": meta_space(&did), "collection": EVENT, "record": record(EVENT, "x")}),
        ),
    ] {
        assert_eq!(
            call(nsid, body).await.error(),
            (403, "ScopeMissing"),
            "{nsid}"
        );
    }
    let said = call("createRecord", json!({"repo": did, "space": "not a space", "collection": EVENT, "record": record(EVENT, "x")})).await;
    assert_eq!(said.error(), (400, "InvalidRequest"));

    // An admin's token, for anything there.
    let app = App::loopback(&format!("atproto space:{CALENDAR}"));
    let access = token(&h, &app, &alice, &did).await;
    let call = async |nsid: &str, body: Json| {
        app.call(
            &h,
            &format!("com.atproto.space.{nsid}"),
            &access,
            Some(&body),
        )
        .await
    };
    let put = json!({"repo": did, "space": calendar, "collection": EVENT, "rkey": rkey, "record": record(EVENT, "changed")});
    assert_eq!(call("putRecord", put.clone()).await.status, 200);
    assert_eq!(call("putRecord", put).await.status, 200);
    let gone = json!({"repo": did, "space": calendar, "collection": EVENT, "rkey": rkey});
    assert_eq!(call("deleteRecord", gone.clone()).await.status, 200);
    assert_eq!(
        h.host
            .store
            .space_record(&did, &calendar, EVENT, rkey)
            .unwrap(),
        None
    );
    assert_eq!(call("deleteRecord", gone).await.body, json!({}));
    // Never the record that says who reads it, nor the index.
    let policy = json!({"repo": did, "space": calendar, "collection": "com.atproto.space.policy", "rkey": "self", "record": {"$type": "com.atproto.space.policy", "policy": {"$type": format!("{NS}.policy"), "grants": []}}});
    assert_eq!(call("putRecord", policy).await.error(), (403, "Forbidden"));
}

#[tokio::test]
async fn writes_in_a_space_as_its_policy_grants_now() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, _, _]) = group(&h).await;
    let scope = format!("atproto space:{CALENDAR}?collection=*");
    let app = App::loopback(&scope);
    let alices = token(&h, &app, &alice, &did).await;
    let body =
        json!({"repo": did, "space": calendar, "collection": EVENT, "record": record(EVENT, "x")});
    let create = "com.atproto.space.createRecord";
    assert_eq!(app.call(&h, create, &alices, Some(&body)).await.status, 200);
    // Once its policy grants admins nothing there, the token writes nothing.
    let readers =
        json!({"$type": format!("{NS}.policy"), "grants": [{"role": "admin", "read": true}]});
    h.host.store.set_policy(&calendar, &readers).unwrap();
    let said = app.call(&h, create, &alices, Some(&body)).await;
    assert_eq!(said.status, 403, "{}", said.body);
}

#[tokio::test]
async fn reads_a_space_with_a_delegation_while_its_member_may() {
    let h = start(&[], false).await;
    let (did, calendar, [_, bob, _]) = group(&h).await;
    let scope = format!("atproto space:{CALENDAR}?action=read space:{NS}.members?action=read");
    let app = App::loopback(&scope);
    let access = token(&h, &app, &bob, &did).await;
    let delegate = |space: String| {
        let nsid = format!("com.atproto.space.getDelegationToken?space={space}");
        let (app, access, h) = (&app, &access, &h);
        async move { app.call(h, &nsid, access, None).await }
    };
    // The group signs it, for this space and the host, its authority.
    let said = delegate(calendar.clone()).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let delegation = said.body["token"].as_str().unwrap();
    let (_, claims) = space_auth::parse(Kind::Delegation, delegation).unwrap();
    assert_eq!(
        (claims.iss.as_str(), claims.sub.as_str()),
        (did.as_str(), calendar.as_str())
    );
    // It buys a credential, which reads the group's repo there.
    let key = Keypair::generate(Curve::P256);
    let ask = json!({"space": calendar, "credentialKey": key.public().did_key(), "delegationToken": delegation});
    let (status, issued) = post(&h, "com.atproto.space.authorizeSpaceSync", &ask, &[]).await;
    assert_eq!(status, 200, "{issued}");
    let authorization = format!(
        "Atproto-Space-Credential {}",
        issued["credential"].as_str().unwrap()
    );
    let mut read = h
        .http
        .get(format!("{}/xrpc/com.atproto.space.listRecords", h.url));
    read = read.query(&[("space", calendar.as_str()), ("repo", did.as_str())]);
    for (k, v) in sign_request(&key, &authorization, &did).unwrap() {
        read = read.header(k, v);
    }
    let res = read.send().await.unwrap();
    assert_eq!(res.status(), 200, "{}", res.text().await.unwrap());
    // None for a space its scope does not read, or one not the group's.
    let said = delegate(meta_space(&did)).await;
    assert_eq!(said.error(), (403, "ScopeMissing"), "{}", said.body);
    let elsewhere = format!("at://did:plc:elsewhere/space/{CALENDAR}/rides");
    assert_eq!(delegate(elsewhere).await.error(), (400, "SpaceNotFound"));
    // Nor once Bob reads the calendar no more: the session reads no more
    // than its member, at each credential.
    let readers =
        json!({"$type": format!("{NS}.policy"), "grants": [{"role": "admin", "read": true}]});
    h.host.store.set_policy(&calendar, &readers).unwrap();
    let said = delegate(calendar.clone()).await;
    assert_eq!(said.error(), (403, "NotAuthorized"), "{}", said.body);
}

#[tokio::test]
async fn configures_the_group_as_its_methods_would() {
    let h = start(&[], false).await;
    let (did, _, [alice, ..]) = group(&h).await;
    let app = App::loopback("atproto include:org.atgrp.authManageGroup");
    let access = token(&h, &app, &alice, &did).await;
    let (meta, members) = (meta_space(&did), members_space(&did));
    let c = |n: &str| format!("{NS}.{n}");
    let put = async |space: &str, collection: &str, rkey: &str, record: Json| {
        app.call(&h, "com.atproto.space.putRecord", &access, Some(&json!({"repo": did, "space": space, "collection": collection, "rkey": rkey, "record": record}))).await
    };
    let delete = async |space: &str, collection: &str, rkey: &str| {
        app.call(
            &h,
            "com.atproto.space.deleteRecord",
            &access,
            Some(&json!({"repo": did, "space": space, "collection": collection, "rkey": rkey})),
        )
        .await
    };
    let mut labeler = h
        .host
        .group_record(&did, &meta, &c("labeler"), "self")
        .unwrap()
        .unwrap();
    labeler["subjectTypes"] = json!(["account"]);
    let profile = |extra: Json| {
        let mut p = json!({"$type": c("profile"), "displayName": "Riders United", "joinPolicy": "open", "createdAt": "2026-11-03T00:00:00Z"});
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        p
    };
    let said = put(&meta, &c("profile"), "self", profile(json!({}))).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let named = h
        .host
        .group_record(&did, &meta, &c("profile"), "self")
        .unwrap()
        .unwrap();
    assert_eq!(named["displayName"], "Riders United");
    let role = json!({"$type": c("role"), "displayName": "Ride leader", "createdAt": "2026-11-03T00:00:00Z"});
    assert_eq!(put(&members, &c("role"), "leader", role).await.status, 200);
    assert_eq!(delete(&members, &c("role"), "leader").await.status, 200);

    for (said, refused) in [
        (
            put(
                &meta,
                &c("profile"),
                "self",
                profile(json!({"displayName": " "})),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            put(
                &meta,
                &c("profile"),
                "self",
                profile(json!({"joinPolicy": "lottery"})),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            put(&meta, &c("profile"), "other", profile(json!({}))).await,
            (400, "InvalidRequest"),
        ),
        // A record of ours that is not what its lexicon describes.
        (
            put(
                &meta,
                &c("rule"),
                "3m2abcdefghij",
                json!({"$type": c("rule"), "title": "x"}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            delete(&meta, &c("profile"), "self").await,
            (400, "InvalidRequest"),
        ),
        // The labeler, likewise, is kept at self, and never taken away,
        // though it may change there.
        (
            put(&meta, &c("labeler"), "self", labeler.clone()).await,
            (200, ""),
        ),
        (
            put(
                &meta,
                &c("labeler"),
                "other",
                json!({"$type": c("labeler")}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            delete(&meta, &c("labeler"), "self").await,
            (400, "InvalidRequest"),
        ),
        (
            delete(&members, &c("role"), "admin").await,
            (400, "RoleInUse"),
        ),
        // What the set doesn't name: a member's own acceptance.
        (
            put(
                &members,
                &c("acceptance"),
                "x",
                json!({"$type": c("acceptance")}),
            )
            .await,
            (403, "ScopeMissing"),
        ),
        (
            put(&meta, &c("access"), "self", json!({"$type": c("access")})).await,
            (403, "ScopeMissing"),
        ),
    ] {
        assert_eq!(said.error(), refused, "{}", said.body);
    }
}

#[test]
fn reads_a_scope_as_atproto_has_it() {
    use atproto::space::SpaceRef;
    use group_host::scopes::Permissions;
    let group = "did:plc:riders";
    let p = Permissions::new(
        "atproto repo?collection=a.b.c&collection=d.e.f&action=create repo:g.h.i?action=delete&action=other repository:x.y.z blob?accept=video/mp4 blob:image/*,text/plain",
        group,
    );
    assert!(p.repo("a.b.c", "create") && p.repo("d.e.f", "create"));
    assert!(!p.repo("a.b.c", "update"));
    assert!(p.repo("g.h.i", "delete") && !p.repo("g.h.i", "create"));
    assert!(!p.repo("x.y.z", "create"));
    assert!(Permissions::new("repo:*", group).repo("any.thing.here", "update"));
    assert!(p.blob("video/mp4") && p.blob("IMAGE/png") && p.blob("text/plain"));
    assert!(!p.blob("text/html") && !p.blob("video/webm"));
    assert!(Permissions::new("blob:*/*", group).blob("application/zip"));
    // Escaped values read as they are meant; an escape of what shapes a
    // scope, or none at all, stays as written.
    let ours = SpaceRef::parse("at://did:plc:riders/space/com.example.calendar/rides").unwrap();
    let p = Permissions::new(
        "space:com.example.calendar?authority=did%3Aplc%3Ariders&collection=a.b%2Ec",
        group,
    );
    assert!(p.space(&ours, Some("a.b.c"), "create"));
    let p = Permissions::new(
        "repo?collection=a.b.c%26action%3Ddelete&action=create%zz%4",
        group,
    );
    assert!(!p.repo("a.b.c", "delete") && !p.repo("a.b.c", "create"));

    let space = |uri: &str| SpaceRef::parse(uri).unwrap();
    let ours = space("at://did:plc:riders/space/com.example.calendar/rides");
    let theirs = space("at://did:plc:other/space/com.example.calendar/rides");
    let p = Permissions::new("space:com.example.calendar?collection=a.b.c", group);
    assert!(p.space(&ours, Some("a.b.c"), "create"));
    assert!(p.space(&ours, None, "read") && p.space(&ours, None, "read_self"));
    assert!(!p.space(&ours, None, "create") && !p.space(&ours, Some("x.y.z"), "create"));
    assert!(!p.space(&theirs, None, "read"));
    let p = Permissions::new(
        "space?spaceType=com.example.calendar&authority=*&skey=other&action=read_self",
        group,
    );
    assert!(!p.space(&theirs, None, "read_self"));
    let p = Permissions::new(
        "space?spaceType=com.example.calendar&authority=*&action=read_self",
        group,
    );
    assert!(p.space(&theirs, None, "read_self") && !p.space(&theirs, None, "read"));
    // The old name of the space type is read no longer.
    let p = Permissions::new("space?type=com.example.calendar&authority=*", group);
    assert!(!p.space(&theirs, None, "read"));
    let p = Permissions::new(
        "space:*?authority=did:plc:other&skey=rides&collection=*&action=delete",
        group,
    );
    assert!(p.space(&theirs, Some("x.y.z"), "delete") && !p.space(&ours, Some("x.y.z"), "delete"));
    assert!(!Permissions::new("space?authority=*", group).space(&ours, None, "read"));
}

#[tokio::test]
async fn asks_everything_of_the_repo_as_it_is_when_written() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    let app = App::loopback(&format!("atproto repo:{POST}?action=create"));
    let access = token(&h, &app, &alice, &did).await;
    let call = async |nsid: &str, body: Json| {
        app.call(
            &h,
            &format!("com.atproto.repo.{nsid}"),
            &access,
            Some(&body),
        )
        .await
    };
    let made = call(
        "createRecord",
        json!({"repo": did, "collection": POST, "rkey": "one", "record": record(POST, "a")}),
    )
    .await;
    assert_eq!(made.status, 200, "{}", made.body);
    // A put of what is there is still an update, and a delete of nothing a
    // delete: neither is this token's.
    let same = json!({"repo": did, "collection": POST, "rkey": "one", "record": record(POST, "a")});
    assert_eq!(call("putRecord", same).await.error(), (403, "ScopeMissing"));
    let none = json!({"repo": did, "collection": POST, "rkey": "none"});
    assert_eq!(
        call("deleteRecord", none).await.error(),
        (403, "ScopeMissing")
    );
    let bad = json!({"repo": did, "collection": POST, "rkey": "two", "record": record(POST, "b"), "swapRecord": 5});
    assert_eq!(
        call("putRecord", bad).await.error(),
        (400, "InvalidRequest")
    );

    // Of two writes that swap the same commit, one lands.
    let app = App::loopback(&format!("atproto repo:{POST}"));
    let access = token(&h, &app, &alice, &did).await;
    let head = h.host.store.head(&did).unwrap().unwrap().cid.to_string();
    let put = |rkey: &str| json!({"repo": did, "collection": POST, "rkey": rkey, "record": record(POST, rkey), "swapCommit": head});
    let (x, y) = (put("x"), put("y"));
    let (a, b) = tokio::join!(
        app.call(&h, "com.atproto.repo.putRecord", &access, Some(&x)),
        app.call(&h, "com.atproto.repo.putRecord", &access, Some(&y)),
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [200, 400], "{} {}", a.body, b.body);
    // A put that changes nothing still swaps: the repo has moved on.
    let same = json!({"repo": did, "collection": POST, "rkey": "one", "record": record(POST, "a"), "swapCommit": head});
    let said = app
        .call(&h, "com.atproto.repo.putRecord", &access, Some(&same))
        .await;
    assert_eq!(said.error(), (400, "InvalidSwap"));
    let gone = json!({"repo": did, "collection": POST, "rkey": "none", "swapCommit": head});
    let said = app
        .call(&h, "com.atproto.repo.deleteRecord", &access, Some(&gone))
        .await;
    assert_eq!(said.error(), (400, "InvalidSwap"));

    // In a space too, a put of what is there is an update.
    let app = App::loopback(&format!(
        "atproto space:{CALENDAR}?collection={EVENT}&action=create"
    ));
    let access = token(&h, &app, &bob, &did).await;
    let event = json!({"repo": did, "space": calendar, "collection": EVENT, "rkey": "ride", "record": record(EVENT, "ride")});
    assert_eq!(
        app.call(&h, "com.atproto.space.putRecord", &access, Some(&event))
            .await
            .status,
        200
    );
    let said = app
        .call(&h, "com.atproto.space.putRecord", &access, Some(&event))
        .await;
    assert_eq!(said.error(), (403, "ScopeMissing"));
}

#[tokio::test]
async fn calls_the_hosts_own_methods_with_an_rpc_scope() {
    let h = start(&[], false).await;
    let (did, _, [alice, bob, carol]) = group(&h).await;
    let methods = [
        "listJoinRequests",
        "denyJoinRequest",
        "listModSubjects",
        "getModSubjectHistory",
        "resolveModSubject",
    ];
    let rpc = |m: &str| format!("rpc:{NS}.{m}?aud=*");
    let mut asked: Vec<String> = methods.iter().map(|m| rpc(m)).collect();
    asked.push(format!("rpc:{INTERNAL}.listInvites?aud=*"));
    asked.push(format!(
        "rpc:{NS}.listJoinRequests?aud=did:web:elsewhere.test"
    ));
    let app = App::loopback(&format!("atproto {}", asked.join(" ")));
    // Approval keeps those Alice's roles may call, for this host or any:
    // never an internal method, nor one for another service.
    let tokens = app.sign_in(&h, &alice, &did).await;
    let mut kept = vec!["atproto".to_string()];
    kept.extend(methods.iter().map(|m| rpc(m)));
    assert_eq!(tokens["scope"], kept.join(" "));
    let access = tokens["access_token"].as_str().unwrap();
    // Bob holds none of their actions, so a grant of his holds none.
    let theirs = app.sign_in(&h, &bob, &did).await;
    assert_eq!(theirs["scope"], "atproto");

    let get = async |q: String| app.call(&h, &q, access, None).await;
    let said = get(format!("{NS}.listJoinRequests?group={did}")).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["requests"], json!([]));
    let deny = json!({"group": did, "member": carol.did});
    let said = app
        .call(&h, &format!("{NS}.denyJoinRequest"), access, Some(&deny))
        .await;
    assert_eq!(said.error(), (400, "JoinRequestNotFound"));
    let said = get(format!("{NS}.listModSubjects?group={did}")).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let history = format!(
        "{NS}.getModSubjectHistory?group={did}&subject={}",
        carol.did
    );
    assert_eq!(get(history.clone()).await.status, 200);
    let resolve = json!({"group": did, "subject": carol.did, "status": "resolved"});
    let said = app
        .call(
            &h,
            &format!("{NS}.resolveModSubject"),
            access,
            Some(&resolve),
        )
        .await;
    assert_eq!(said.error(), (400, "SubjectNotFound"));

    // Or for this host by name, or through one of its permission sets.
    let ours = &h.host.config.service_did;
    let named = format!("atproto rpc:{NS}.listModSubjects?aud={ours}");
    for scope in [named.as_str(), "atproto include:org.atgrp.authModerate"] {
        let app = App::loopback(scope);
        let access = token(&h, &app, &alice, &did).await;
        let nsid = format!("{NS}.listModSubjects?group={did}");
        let said = app.call(&h, &nsid, &access, None).await;
        assert_eq!(said.status, 200, "{scope}: {}", said.body);
    }

    // Not without the scope, nor for another group.
    let other = App::loopback(&format!("atproto space:{CALENDAR}?action=read"));
    let without = token(&h, &other, &alice, &did).await;
    let said = other
        .call(
            &h,
            &format!("{NS}.listModSubjects?group={did}"),
            &without,
            None,
        )
        .await;
    assert_eq!(said.error(), (403, "ScopeMissing"));
    let said = get(format!("{NS}.listJoinRequests?group=did:plc:elsewhere")).await;
    assert_eq!(said.error(), (403, "Forbidden"));

    // Asked again at each call: once Alice admits nobody, nor does it.
    membership(&h, &did, &bob.did, &["member", "admin"]).await;
    membership(&h, &did, &alice.did, &["member"]).await;
    let said = get(format!("{NS}.listJoinRequests?group={did}")).await;
    assert_eq!(said.error(), (403, "Forbidden"));
    assert_eq!(get(history).await.error(), (403, "Forbidden"));
}

/// A delegation token for `space` from `app`'s session `access`.
async fn delegation(h: &Running, app: &App, access: &str, space: &str) -> String {
    let nsid = format!("com.atproto.space.getDelegationToken?space={space}");
    let said = app.call(h, &nsid, access, None).await;
    assert_eq!(said.status, 200, "{}", said.body);
    said.body["token"].as_str().unwrap().to_string()
}

/// What `delegation` buys of `space`: a credential, bound to a new key,
/// and, with `notify`, a syncer told of the space's changes; the
/// status and error, the credential and its key.
async fn buy(
    h: &Running,
    space: &str,
    delegation: &str,
    notify: Option<Json>,
) -> ((u16, String), String, Keypair) {
    let key = Keypair::generate(Curve::P256);
    let mut ask = json!({"space": space, "credentialKey": key.public().did_key(), "delegationToken": delegation});
    if let Some(notify) = notify {
        ask["notify"] = notify;
    }
    let (status, issued) = post(h, "com.atproto.space.authorizeSpaceSync", &ask, &[]).await;
    let error = issued["error"].as_str().unwrap_or_default().to_string();
    let credential = issued["credential"].as_str().unwrap_or_default();
    ((status, error), credential.to_string(), key)
}

/// What reading the group's repo in `space` with `credential` answers.
async fn read_with(h: &Running, did: &str, space: &str, credential: &str, key: &Keypair) -> u16 {
    let authorization = format!("Atproto-Space-Credential {credential}");
    let mut read = h
        .http
        .get(format!("{}/xrpc/com.atproto.space.listRecords", h.url));
    read = read.query(&[("space", space), ("repo", did)]);
    for (k, v) in sign_request(key, &authorization, did).unwrap() {
        read = read.header(k, v);
    }
    read.send().await.unwrap().status().as_u16()
}

/// An event the group writes in `space`, which its syncers are told of.
async fn event(h: &Running, did: &str, space: &str, rkey: &str) {
    let write = Write::Create {
        collection: EVENT.into(),
        rkey: rkey.into(),
        value: record(EVENT, "Night ride"),
    };
    h.host.space_write(did, space, &[write]).await.unwrap();
}

#[tokio::test]
async fn what_a_grant_bought_ends_when_it_is_revoked() {
    // Over http, as its syncer's endpoint is.
    let h = start_reachable(&[]).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    let app = App::loopback(&format!("atproto space:{CALENDAR}?action=read"));
    let access = token(&h, &app, &bob, &did).await;
    let (url, heard) = endpoint().await;
    let notify = json!({"endpoint": url, "token": "bobs-app"});
    let first = delegation(&h, &app, &access, &calendar).await;
    let (status, credential, key) = buy(&h, &calendar, &first, Some(notify)).await;
    assert_eq!(status, (200, String::new()));
    let spare = delegation(&h, &app, &access, &calendar).await;
    assert_eq!(read_with(&h, &did, &calendar, &credential, &key).await, 200);
    // Alice signs the app in too, and her grant buys its own.
    let hers = token(&h, &app, &alice, &did).await;
    let (url, heard_hers) = endpoint().await;
    let notify = json!({"endpoint": url, "token": "alices-app"});
    let bought = delegation(&h, &app, &hers, &calendar).await;
    let (status, kept, kept_key) = buy(&h, &calendar, &bought, Some(notify)).await;
    assert_eq!(status, (200, String::new()));
    event(&h, &did, &calendar, "one").await;
    heard.told(1).await;
    heard_hers.told(1).await;

    let grants = h.host.store.grants_of(&did, 0).unwrap();
    let grant = grants.iter().find(|g| g.member == bob.did).unwrap();
    let lxm = format!("{INTERNAL}.revokeGroupToken");
    let body = json!({"group": did, "tokenId": grant.id});
    let (status, said) = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(status, 200, "{said}");

    // At once: its credential reads nothing, a delegation it was given
    // buys nothing, and its syncer is told nothing more.
    assert_eq!(read_with(&h, &did, &calendar, &credential, &key).await, 401);
    let refused = buy(&h, &calendar, &spare, None).await.0;
    assert_eq!(refused, (400, "SpaceNotFound".to_string()));
    event(&h, &did, &calendar, "two").await;
    assert_eq!(heard.told_by_now().await.len(), 1);
    // What Alice's grant bought lasts.
    assert_eq!(read_with(&h, &did, &calendar, &kept, &kept_key).await, 200);
    heard_hers.told(2).await;
}

#[tokio::test]
async fn what_a_grant_bought_ends_once_its_member_reads_no_more() {
    let h = start_reachable(&[]).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    // The calendar is for its ride leaders; Bob leads rides.
    let leaders = json!({"$type": format!("{NS}.policy"), "grants": [
        {"role": "admin", "read": true, "write": true, "scopes": [format!("space:{CALENDAR}?collection=*")]},
        {"role": "leader", "read": true},
    ]});
    h.host.store.set_policy(&calendar, &leaders).unwrap();
    let role = Write::Create {
        collection: format!("{NS}.role"),
        rkey: "leader".into(),
        value: json!({"$type": format!("{NS}.role"), "displayName": "Ride leader", "createdAt": "2026-11-03T00:00:00Z"}),
    };
    h.host
        .space_write(&did, &members_space(&did), &[role])
        .await
        .unwrap();
    membership(&h, &did, &bob.did, &["member", "leader"]).await;

    let app = App::loopback(&format!("atproto space:{CALENDAR}?action=read"));
    let access = token(&h, &app, &bob, &did).await;
    let (url, heard) = endpoint().await;
    let notify = json!({"endpoint": url, "token": "bobs-app"});
    let first = delegation(&h, &app, &access, &calendar).await;
    let (status, credential, key) = buy(&h, &calendar, &first, Some(notify)).await;
    assert_eq!(status, (200, String::new()));
    let spare = delegation(&h, &app, &access, &calendar).await;
    event(&h, &did, &calendar, "one").await;
    heard.told(1).await;

    // Alice, in her console, makes Bob a member alone.
    let admin = App::loopback("atproto include:org.atgrp.authManageGroup");
    let theirs = token(&h, &admin, &alice, &did).await;
    let membership = json!({"repo": did, "space": members_space(&did), "collection": format!("{NS}.membership"), "rkey": bob.did,
        "record": {"$type": format!("{NS}.membership"), "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}});
    let said = admin
        .call(
            &h,
            "com.atproto.space.putRecord",
            &theirs,
            Some(&membership),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);

    // The grant lasts, but reads the calendar no more, at once.
    assert_eq!(read_with(&h, &did, &calendar, &credential, &key).await, 401);
    let refused = buy(&h, &calendar, &spare, None).await.0;
    assert_eq!(refused, (400, "SpaceNotFound".to_string()));
    event(&h, &did, &calendar, "two").await;
    assert_eq!(heard.told_by_now().await.len(), 1);
    let said = app
        .call(&h, "com.atproto.server.getSession", &access, None)
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
}
