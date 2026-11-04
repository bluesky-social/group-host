//! Using a token: the resource server's checks and challenges, and writing
//! as the group in its repo and spaces, as far as the token's scope and
//! its member's roles reach.

mod common;

use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::{members_space, meta_space};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const POST: &str = "app.bsky.feed.post";
const CALENDAR: &str = "com.example.calendar";
const EVENT: &str = "com.example.event";

async fn membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let roles: Vec<Json> = roles.iter().map(|r| json!({"role": r})).collect();
    let value = json!({"$type": format!("{NS}.membership"), "member": who, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
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
    let lxm = format!("{NS}.createSpace");
    let body = json!({
        "group": did,
        "spaceType": CALENDAR,
        "skey": "rides",
        "readableBy": [{"role": "member"}],
        "credentialScopes": [
            {"role": "member", "scopes": [{"collection": format!("{EVENT}?action=create")}]},
            {"role": "admin", "scopes": [{"collection": "*"}]},
        ],
    });
    let (status, made) = post(h, &lxm, &body, &[auth(h, &alice, &lxm)]).await;
    assert_eq!(status, 200, "{made}");
    membership(h, &did, &bob.did, &["member"]).await;
    (
        did,
        made["uri"].as_str().unwrap().to_string(),
        [alice, bob, carol],
    )
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
    let lxm = format!("{NS}.deactivateGroup");
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
        "atproto repo:{POST} include:fyi.opensocial.configure space:{CALENDAR}?collection=*"
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
        app.call(&h, "com.atproto.space.createRecord", &access, Some(&json!({"repo": did, "space": meta_space(&did), "collection": format!("{NS}.rule"), "record": {"$type": format!("{NS}.rule"), "title": "Be kind", "createdAt": "2026-11-03T00:00:00Z"}}))).await
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
    // Never the record that says who reads it.
    let access_record = json!({"repo": did, "space": calendar, "collection": format!("{NS}.access"), "rkey": "self", "record": {"$type": format!("{NS}.access"), "readableBy": [], "createdAt": "2026-11-03T00:00:00Z"}});
    assert_eq!(
        call("putRecord", access_record).await.error(),
        (403, "Forbidden")
    );
}

#[tokio::test]
async fn in_a_space_for_chosen_people_writes_for_those_of_them_who_may_do_anything() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    membership(&h, &did, &bob.did, &["member", "admin"]).await;
    let scope = format!("atproto space:{CALENDAR}?collection=*");
    let app = App::loopback(&scope);
    // Both admins sign the app in while the calendar is the members'.
    let alices = token(&h, &app, &alice, &did).await;
    let bobs = token(&h, &app, &bob, &did).await;
    let access = json!({"$type": format!("{NS}.access"), "readableBy": [], "people": [{"did": alice.did}], "createdAt": "2026-11-03T00:00:00Z"});
    let write = Write::Update {
        collection: format!("{NS}.access"),
        rkey: "self".into(),
        value: access,
    };
    h.host.space_write(&did, &calendar, &[write]).await.unwrap();
    let body =
        json!({"repo": did, "space": calendar, "collection": EVENT, "record": record(EVENT, "x")});
    let create = "com.atproto.space.createRecord";
    assert_eq!(app.call(&h, create, &alices, Some(&body)).await.status, 200);
    // Bob is not one of its people now.
    let said = app.call(&h, create, &bobs, Some(&body)).await;
    assert_eq!(said.status, 403, "{}", said.body);
}

#[tokio::test]
async fn configures_the_group_as_its_methods_would() {
    let h = start(&[], false).await;
    let (did, _, [alice, ..]) = group(&h).await;
    let app = App::loopback("atproto include:fyi.opensocial.configure");
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
    let profile = |extra: Json| {
        let mut p = json!({"$type": c("profile"), "displayName": "Riders United", "createdAt": "2026-11-03T00:00:00Z"});
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
    let role =
        json!({"$type": c("role"), "name": "Ride leader", "createdAt": "2026-11-03T00:00:00Z"});
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
        (
            delete(&meta, &c("profile"), "self").await,
            (400, "InvalidRequest"),
        ),
        (
            delete(&members, &c("role"), "admin").await,
            (400, "RoleInUse"),
        ),
        (
            put(
                &members,
                &c("membership"),
                "x",
                json!({"$type": c("membership")}),
            )
            .await,
            (403, "ScopeMissing"),
        ),
        (
            put(
                &members,
                &c("permissions"),
                "self",
                json!({"$type": c("permissions")}),
            )
            .await,
            (403, "Forbidden"),
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

    let space = |uri: &str| SpaceRef::parse(uri).unwrap();
    let ours = space("at://did:plc:riders/space/com.example.calendar/rides");
    let theirs = space("at://did:plc:other/space/com.example.calendar/rides");
    let p = Permissions::new("space:com.example.calendar?collection=a.b.c", group);
    assert!(p.space(&ours, Some("a.b.c"), "create"));
    assert!(p.space(&ours, None, "read") && p.space(&ours, None, "read_self"));
    assert!(!p.space(&ours, None, "create") && !p.space(&ours, Some("x.y.z"), "create"));
    assert!(!p.space(&theirs, None, "read"));
    let p = Permissions::new(
        "space?type=com.example.calendar&authority=*&skey=other&action=read_self",
        group,
    );
    assert!(!p.space(&theirs, None, "read_self"));
    let p = Permissions::new(
        "space?type=com.example.calendar&authority=*&action=read_self",
        group,
    );
    assert!(p.space(&theirs, None, "read_self") && !p.space(&theirs, None, "read"));
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
