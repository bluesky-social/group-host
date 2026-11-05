//! Acting as the group from the console with a group token, and seeing and
//! ending the grants that act as it, with what they did.

mod common;

use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::members_space;
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const CALENDAR: &str = "com.example.calendar";
const EVENT: &str = "com.example.event";
const GET_GROUP_AUTH: &str = "fyi.opensocial.getGroupAuth";

async fn membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let value = json!({"$type": format!("{NS}.membership"), "member": who, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let write = Write::Create {
        collection: format!("{NS}.membership"),
        rkey: who.into(),
        value,
    };
    h.host
        .space_write(did, &members_space(did), &[write])
        .await
        .unwrap();
}

/// Riders, Alice's, with a calendar where members add events as the group
/// and admins write anything; Bob a member, Carol not one.
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
        "type": CALENDAR,
        "skey": "rides",
        "access": {"grants": [
            {"role": "member", "scopes": [format!("space:{CALENDAR}?action=read&collection={EVENT}&action=create")]},
            {"role": "admin", "scopes": [format!("space:{CALENDAR}?collection=*")]},
        ]},
    });
    let (status, made) = post(h, &lxm, &body, &[auth(h, &alice, &lxm)]).await;
    assert_eq!(status, 200, "{made}");
    membership(h, &did, &bob.did, &["member"]).await;
    (
        did,
        made["space"].as_str().unwrap().to_string(),
        [alice, bob, carol],
    )
}

/// Asks for a group token as `who`, proved by `app`'s key, once more with
/// the nonce if asked.
async fn group_auth(h: &Running, app: &App, who: &Authority, body: &Json) -> Said {
    let send = || {
        let (k, v) = auth(h, who, GET_GROUP_AUTH);
        h.http
            .post(format!("{}/xrpc/{GET_GROUP_AUTH}", h.url))
            .json(body)
            .header(k, v)
            .header(
                "dpop",
                app.proof_with(h, "POST", &format!("/xrpc/{GET_GROUP_AUTH}"), |_| {}),
            )
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

fn record(collection: &str) -> Json {
    json!({"$type": collection, "text": "ride", "createdAt": "2026-11-03T00:00:00Z"})
}

#[tokio::test]
async fn gives_a_member_a_token_for_what_their_roles_write_in_a_space() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    let app = App::new("console");
    let said = group_auth(&h, &app, &bob, &json!({"group": did, "space": calendar})).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let body = &said.body;
    assert_eq!(body["did"], did.as_str());
    assert_eq!(body["pds"], "https://host.test");
    assert_eq!(body["collections"], json!([{"collection": EVENT}]));
    assert_eq!(body["repoCollections"], json!([{"collection": EVENT}]));
    let scope = body["scope"].as_str().unwrap();
    assert_eq!(
        scope,
        format!(
            "atproto space:{CALENDAR}?authority={did}&skey=rides&collection={EVENT}&action=read&action=create repo:{EVENT}?action=create blob:image/*"
        )
    );
    assert!(body["expiresAt"].is_string());
    let token = body["accessJwt"].as_str().unwrap();

    // It writes events, in the calendar and the group's repo, and no more.
    let in_space =
        json!({"repo": did, "space": calendar, "collection": EVENT, "record": record(EVENT)});
    let said = app
        .call(&h, "com.atproto.space.createRecord", token, Some(&in_space))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let in_repo = json!({"repo": did, "collection": EVENT, "record": record(EVENT)});
    let said = app
        .call(&h, "com.atproto.repo.createRecord", token, Some(&in_repo))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let post = json!({"repo": did, "collection": "app.bsky.feed.post", "record": record("app.bsky.feed.post")});
    let said = app
        .call(&h, "com.atproto.repo.createRecord", token, Some(&post))
        .await;
    assert_eq!(said.error(), (403, "ScopeMissing"));

    // An admin's is for anything there, and any file.
    let said = group_auth(&h, &app, &alice, &json!({"group": did, "space": calendar})).await;
    assert_eq!(
        said.body["collections"],
        json!([{"collection": "*"}, {"collection": EVENT}])
    );
    assert!(said.body["scope"].as_str().unwrap().ends_with("blob:*/*"));
}

#[tokio::test]
async fn gives_a_member_a_token_for_the_members_space_as_the_group_grants_them() {
    let h = start(&[], false).await;
    let (did, _, [alice, bob, _]) = group(&h).await;
    let members = members_space(&did);
    let app = App::new("console");
    let said = group_auth(&h, &app, &alice, &json!({"group": did, "space": members})).await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Roles, the permissions record and memberships, kept there alone.
    let c = |n: &str| format!("{NS}.{n}");
    let names: Vec<Json> = ["membership", "permissions", "role"]
        .iter()
        .map(|n| json!({"collection": c(n)}))
        .collect();
    assert_eq!(said.body["collections"], json!(names));
    assert_eq!(said.body["repoCollections"], json!([]));
    let scope = said.body["scope"].as_str().unwrap();
    let at = format!("space:{NS}.members?authority={did}&skey=self");
    let all = "action=read&action=create&action=delete&action=update";
    assert!(
        scope.contains(&format!("{at}&collection={}&{all}", c("role"))),
        "{scope}"
    );
    assert!(!scope.contains("repo:"), "{scope}");
    let token = said.body["accessJwt"].as_str().unwrap();
    let role = json!({"$type": c("role"), "displayName": "Ride leader"});
    let body = json!({"repo": did, "space": members, "collection": c("role"), "rkey": "leader", "record": role});
    let said = app
        .call(&h, "com.atproto.space.putRecord", token, Some(&body))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // A member the group grants nothing writes nothing there.
    let said = group_auth(&h, &app, &bob, &json!({"group": did, "space": members})).await;
    assert_eq!(said.error(), (403, "Forbidden"));
}

#[tokio::test]
async fn refuses_a_group_token_it_should_not_give() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, _, carol]) = group(&h).await;
    let app = App::new("console");
    let ask = async |who: &Authority, body: Json| group_auth(&h, &app, who, &body).await;
    let refused = [
        (ask(&carol, json!({"group": did, "space": calendar})).await, (403, "Forbidden")),
        (ask(&alice, json!({"group": did, "space": format!("at://{}/space/{CALENDAR}/rides", alice.did)})).await, (400, "InvalidRequest")),
        (ask(&alice, json!({"group": did, "space": "nonsense"})).await, (400, "InvalidRequest")),
        (ask(&alice, json!({"group": "did:plc:nothing", "space": calendar})).await, (400, "GroupNotFound")),
        (ask(&alice, json!({"space": calendar})).await, (400, "InvalidRequest")),
        (ask(&alice, json!({"group": did})).await, (400, "InvalidRequest")),
    ];
    for (said, error) in refused {
        assert_eq!(said.error(), error, "{}", said.body);
    }
    // Without a proof, or service auth.
    let (k, v) = auth(&h, &alice, GET_GROUP_AUTH);
    let bare = h
        .http
        .post(format!("{}/xrpc/{GET_GROUP_AUTH}", h.url))
        .json(&json!({"group": did, "space": calendar}));
    assert_eq!(
        app.said(bare.header(k, v)).await.error(),
        (401, "invalid_dpop_proof")
    );
    let said = app
        .said(
            h.http
                .post(format!("{}/xrpc/{GET_GROUP_AUTH}", h.url))
                .json(&json!({"group": did, "space": calendar})),
        )
        .await;
    assert_eq!(said.error(), (401, "AuthMissing"));
    // Nor while the group is switched off.
    let lxm = format!("{NS}.deactivateGroup");
    post(&h, &lxm, &json!({"group": did}), &[auth(&h, &alice, &lxm)]).await;
    let said = ask(&alice, json!({"group": did, "space": calendar})).await;
    assert_eq!(said.error(), (400, "RepoDeactivated"));
}

#[tokio::test]
async fn shows_its_configurers_who_acts_as_the_group_and_lets_them_end_it() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    let app = App::loopback("atproto repo:app.bsky.feed.post");
    let tokens = app.sign_in(&h, &alice, &did).await;
    let token = tokens["access_token"].as_str().unwrap();
    let write = json!({"repo": did, "collection": "app.bsky.feed.post", "rkey": "one", "record": record("app.bsky.feed.post")});
    assert_eq!(
        app.call(&h, "com.atproto.repo.createRecord", token, Some(&write))
            .await
            .status,
        200
    );
    let console = App::new("console");
    let given = group_auth(
        &h,
        &console,
        &bob,
        &json!({"group": did, "space": calendar}),
    )
    .await;
    assert_eq!(given.status, 200, "{}", given.body);

    let lxm = format!("{NS}.listGroupTokens");
    let list = async |who: &Authority, group: &str| {
        get(&h, &lxm, &format!("group={group}"), &[auth(&h, who, &lxm)]).await
    };
    let (status, listed) = list(&alice, &did).await;
    assert_eq!(status, 200, "{listed}");
    let sessions = listed["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    let oauth = sessions.iter().find(|s| s["source"] == "oauth").unwrap();
    assert_eq!(oauth["clientId"], app.id.as_str());
    assert_eq!(oauth["member"], alice.did.as_str());
    assert_eq!(oauth["scope"], "atproto repo:app.bsky.feed.post");
    assert!(oauth["lastUsedAt"].is_string() && oauth["grantedAt"].is_string());
    let groups = sessions
        .iter()
        .find(|s| s["source"] == "getGroupAuth")
        .unwrap();
    assert_eq!(groups["member"], bob.did.as_str());
    assert_eq!(groups.get("lastUsedAt"), None);
    let actions: Vec<&str> = listed["activity"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        [
            format!("took a token for {calendar}"),
            "created app.bsky.feed.post/one".to_string(),
            format!("signed in {}", app.id),
        ]
    );
    assert_eq!(listed["activity"][1]["clientId"], app.id.as_str());

    // Only for its configurers.
    assert_eq!(list(&bob, &did).await.0, 403);
    assert_eq!(list(&alice, "did:plc:nothing").await.0, 400);

    let lxm = format!("{NS}.revokeGroupToken");
    let revoke =
        async |who: &Authority, body: Json| post(&h, &lxm, &body, &[auth(&h, who, &lxm)]).await;
    let id = oauth["tokenId"].as_str().unwrap();
    assert_eq!(
        revoke(&bob, json!({"group": did, "tokenId": id})).await.0,
        403
    );
    assert_eq!(revoke(&alice, json!({"group": did})).await.0, 400);
    assert_eq!(revoke(&alice, json!({"tokenId": id})).await.0, 400);
    assert_eq!(
        revoke(&alice, json!({"group": did, "tokenId": "tok-nothing"}))
            .await
            .1["error"],
        "NotFound"
    );
    let (status, done) = revoke(&alice, json!({"group": did, "tokenId": id})).await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(
        app.call(&h, "com.atproto.repo.createRecord", token, Some(&write))
            .await
            .error(),
        (401, "invalid_token")
    );
    let (_, listed) = list(&alice, &did).await;
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(
        listed["activity"][0]["action"],
        format!("revoked {}'s grant to {}", alice.did, app.id)
    );

    // Not another group's.
    let other = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let theirs = groups["tokenId"].as_str().unwrap();
    let said = revoke(&alice, json!({"group": other, "tokenId": theirs})).await;
    assert_eq!(said.1["error"], "NotFound");
}

#[tokio::test]
async fn publishes_only_as_the_role_may_write_in_the_space() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, _]) = group(&h).await;
    let access_of = |scope: String| {
        json!({
            "$type": format!("{NS}.access"),
            "grants": [{"role": "member", "scopes": [format!("space:{CALENDAR}?action=read"), scope]}],
        })
    };
    let set = async |access: Json| {
        let write = Write::Update {
            collection: format!("{NS}.access"),
            rkey: "self".into(),
            value: access,
        };
        h.host.space_write(&did, &calendar, &[write]).await.unwrap();
    };
    // Bob may write events every way, and takes a token while he may.
    set(access_of(format!("space:{CALENDAR}?collection={EVENT}"))).await;
    let app = App::new("console");
    let said = group_auth(&h, &app, &bob, &json!({"group": did, "space": calendar})).await;
    let token = said.body["accessJwt"].as_str().unwrap().to_string();
    let put = |text: &str| json!({"repo": did, "collection": EVENT, "rkey": "ride", "record": {"$type": EVENT, "text": text, "createdAt": "2026-11-03T00:00:00Z"}});
    let said = app
        .call(&h, "com.atproto.repo.putRecord", &token, Some(&put("one")))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Now only add them: in the repo too.
    set(access_of(format!(
        "space:{CALENDAR}?collection={EVENT}&action=create"
    )))
    .await;
    let said = app
        .call(&h, "com.atproto.repo.putRecord", &token, Some(&put("two")))
        .await;
    assert_eq!(said.status, 403, "{}", said.body);
    assert!(
        said.body["message"]
            .as_str()
            .unwrap()
            .contains("may no longer write")
    );

    // The log says what each write was, as it was made.
    let lxm = format!("{NS}.listGroupTokens");
    let (_, listed) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(
        listed["activity"][0]["action"],
        format!("created {EVENT}/ride")
    );
}

#[tokio::test]
async fn in_a_space_for_chosen_people_gives_those_of_them_who_may_do_anything() {
    let h = start(&[], false).await;
    let (did, calendar, [alice, bob, carol]) = group(&h).await;
    membership(&h, &did, &carol.did, &["admin"]).await;
    // Alice and Bob its people, members given events though its people
    // decide; Carol an admin but not one of them.
    let access = json!({
        "$type": format!("{NS}.access"),
        "grants": [{"role": "member", "scopes": [format!("space:{CALENDAR}?collection={EVENT}")]}],
        "people": [{"did": alice.did}, {"did": bob.did}],
    });
    let write = Write::Update {
        collection: format!("{NS}.access"),
        rkey: "self".into(),
        value: access,
    };
    h.host.space_write(&did, &calendar, &[write]).await.unwrap();
    let app = App::new("console");
    let body = json!({"group": did, "space": calendar});
    let said = group_auth(&h, &app, &alice, &body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["collections"], json!([{"collection": "*"}]));
    let token = said.body["accessJwt"].as_str().unwrap();
    let in_space =
        json!({"repo": did, "space": calendar, "collection": EVENT, "record": record(EVENT)});
    let said = app
        .call(&h, "com.atproto.space.createRecord", token, Some(&in_space))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Bob is one of its people, but may not do anything as the group;
    // Carol may, but is not one of them.
    for who in [&bob, &carol] {
        let said = group_auth(&h, &app, who, &body).await;
        assert_eq!(said.error(), (403, "Forbidden"), "{}", said.body);
    }
}
