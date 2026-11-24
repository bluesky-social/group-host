//! Signing an app in as a group, from the member's side: the groups the
//! approval page offers and what the app would get in each, approving,
//! and declining.

mod common;

use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use serde_json::{Value as Json, json};

const SCOPE: &str = "atproto repo:app.bsky.feed.post include:fyi.opensocial.authManageGroup include:fyi.opensocial.authModerate";

async fn join(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let membership = json!({"$type": format!("{NS}.membership"), "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let write = group_host::repo::Write::Create {
        collection: format!("{NS}.membership"),
        rkey: who.into(),
        value: membership,
    };
    let space = group_host::group::members_space(did);
    h.host.space_write(did, &space, &[write]).await.unwrap();
}

/// Riders and Hikers, both Alice's; Bob a member of Riders, Carol of
/// neither. Hikers' profile is for its members alone.
async fn groups(h: &Running) -> (String, String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let riders = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let hikers = start_group(
        h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers", "metaPublicRead": false}),
    )
    .await;
    join(h, &riders, &bob.did, &["member"]).await;
    (riders, hikers, [alice, bob, carol])
}

async fn offer(h: &Running, who: &Authority, request_uri: &str) -> (u16, Json) {
    let lxm = format!("{INTERNAL}.getAuthorization");
    let query = format!("requestUri={}", urlencode(request_uri));
    get(h, &lxm, &query, &[auth(h, who, &lxm)]).await
}

fn urlencode(text: &str) -> String {
    reqwest::Url::parse_with_params("http://x/", &[("q", text)])
        .unwrap()
        .query()
        .unwrap()[2..]
        .to_string()
}

fn scopes(list: &Json) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|s| s["scope"].as_str().unwrap())
        .collect()
}

fn dids(offered: &Json) -> Vec<&str> {
    offered["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["did"].as_str().unwrap())
        .collect()
}

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = nsid(method);
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

#[tokio::test]
async fn offers_the_groups_where_the_caller_holds_a_role_with_what_the_app_would_get() {
    let h = start(&[], false).await;
    let (riders, hikers, [alice, bob, carol]) = groups(&h).await;
    let app = App::loopback(SCOPE);
    let request_uri = app.push(&h, &[]).await;
    // Hikers' profile gone, so it has no name to show.
    drop_profile(&h, &hikers).await;

    let (status, offered) = offer(&h, &alice, &request_uri).await;
    assert_eq!(status, 200, "{offered}");
    assert_eq!(offered["client"]["clientId"], app.id.as_str());
    assert_eq!(offered["client"]["name"], "An app on this device");
    assert_eq!(
        scopes(&offered["requested"]),
        SCOPE.split(' ').collect::<Vec<_>>()
    );
    assert_eq!(
        offered["labels"],
        json!([
            {"scope": "include:fyi.opensocial.authManageGroup", "title": "Group administration", "text": "Manage everything in the group's info and member list, including its roles and what each role can do."},
            {"scope": "include:fyi.opensocial.authModerate", "title": "Group moderation", "text": "Approve, invite, label, and remove members, change their roles, and review reports."},
        ])
    );
    let mut theirs = dids(&offered);
    theirs.sort();
    let mut both = vec![riders.as_str(), hikers.as_str()];
    both.sort();
    assert_eq!(theirs, both);
    let at_riders = offered["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["did"] == riders.as_str())
        .unwrap();
    assert_eq!(at_riders["handle"], RIDERS);
    assert_eq!(at_riders["displayName"], "Riders");
    assert_eq!(
        at_riders["roles"],
        json!([{"role": "admin"}, {"role": "member"}])
    );
    assert_eq!(
        scopes(&at_riders["granted"]),
        SCOPE.split(' ').collect::<Vec<_>>()
    );
    assert_eq!(at_riders["withheld"], json!([]));
    let at_hikers = offered["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["did"] == hikers.as_str())
        .unwrap();
    assert_eq!(at_hikers["handle"], "hikers.groups.test");
    assert_eq!(at_hikers.get("displayName"), None);

    let (_, offered) = offer(&h, &bob, &request_uri).await;
    assert_eq!(dids(&offered), [riders.as_str()]);
    let at_riders = &offered["groups"][0];
    assert_eq!(scopes(&at_riders["granted"]), ["atproto"]);
    assert_eq!(
        scopes(&at_riders["withheld"]),
        [
            "repo:app.bsky.feed.post",
            "include:fyi.opensocial.authManageGroup",
            "include:fyi.opensocial.authModerate"
        ]
    );

    let (_, offered) = offer(&h, &carol, &request_uri).await;
    assert_eq!(offered["groups"], json!([]));

    // Not a group switched off.
    let (status, off) = call(&h, &alice, "deactivateGroup", json!({"group": hikers})).await;
    assert_eq!(status, 200, "{off}");
    let (_, offered) = offer(&h, &alice, &request_uri).await;
    assert_eq!(dids(&offered), [riders.as_str()]);
}

#[tokio::test]
async fn offers_the_tier_a_moderator_holds_of_the_one_asked() {
    let h = start(&[], false).await;
    let (riders, _, [_, bob, _]) = groups(&h).await;
    let role = json!({"$type": format!("{NS}.role"), "displayName": "Moderator", "scopes": [format!("include:{NS}.authModerate")], "createdAt": "2026-11-03T00:00:00Z"});
    let write = group_host::repo::Write::Create {
        collection: format!("{NS}.role"),
        rkey: "moderator".into(),
        value: role,
    };
    let space = group_host::group::members_space(&riders);
    h.host.space_write(&riders, &space, &[write]).await.unwrap();
    let write = group_host::repo::Write::Update {
        collection: format!("{NS}.membership"),
        rkey: bob.did.clone(),
        value: json!({"$type": format!("{NS}.membership"), "roles": ["member", "moderator"], "createdAt": "2026-11-03T00:00:00Z"}),
    };
    h.host.space_write(&riders, &space, &[write]).await.unwrap();
    let app = App::loopback("atproto include:fyi.opensocial.authManageGroup");
    let request_uri = app.push(&h, &[]).await;
    let (status, offered) = offer(&h, &bob, &request_uri).await;
    assert_eq!(status, 200, "{offered}");
    let at_riders = &offered["groups"][0];
    assert_eq!(
        scopes(&at_riders["granted"]),
        ["atproto", "include:fyi.opensocial.authModerate"]
    );
    assert_eq!(
        scopes(&at_riders["withheld"]),
        ["include:fyi.opensocial.authManageGroup"]
    );
    // Both named, for the page to say what each is.
    assert_eq!(
        scopes(&offered["labels"]),
        [
            "include:fyi.opensocial.authManageGroup",
            "include:fyi.opensocial.authModerate"
        ]
    );
}

#[tokio::test]
async fn offers_only_the_group_the_app_named_even_to_one_without_a_role() {
    let h = start(&[], false).await;
    let (riders, hikers, [alice, _, carol]) = groups(&h).await;
    let app = App::loopback(SCOPE);
    for hint in [
        RIDERS.to_string(),
        format!("@{}", RIDERS.to_uppercase()),
        format!("at://{riders}"),
        riders.clone(),
    ] {
        let request_uri = app.push(&h, &[("login_hint", &hint)]).await;
        let (_, offered) = offer(&h, &alice, &request_uri).await;
        assert_eq!(dids(&offered), [riders.as_str()], "{hint}");
        assert_eq!(offered["loginHint"], hint.as_str());
        let (_, offered) = offer(&h, &carol, &request_uri).await;
        assert_eq!(offered["groups"][0]["roles"], json!([]));
        assert_eq!(offered["groups"][0]["granted"], json!([]));
        assert_eq!(offered["groups"][0]["displayName"], "Riders");
    }
    // Its profile only for whoever may read it.
    let request_uri = app.push(&h, &[("login_hint", "hikers.groups.test")]).await;
    let (_, offered) = offer(&h, &carol, &request_uri).await;
    assert_eq!(dids(&offered), [hikers.as_str()]);
    assert_eq!(offered["groups"][0].get("displayName"), None);
    // A name that is no group here is no help.
    let request_uri = app.push(&h, &[("login_hint", "nobody.test")]).await;
    let (_, offered) = offer(&h, &alice, &request_uri).await;
    assert_eq!(offered["groups"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn shows_the_groups_picture() {
    let h = start(&[], false).await;
    let (riders, _, [alice, ..]) = groups(&h).await;
    let image = give_avatar(&h, &alice, &riders, "Riders", b"face").await;
    let request_uri = App::loopback(SCOPE)
        .push(&h, &[("login_hint", RIDERS)])
        .await;
    let (_, offered) = offer(&h, &alice, &request_uri).await;
    let avatar = offered["groups"][0]["avatar"].as_str().unwrap();
    let cid = image["ref"]["$link"].as_str().unwrap();
    assert_eq!(param(avatar, "cid").unwrap(), cid);
    // Where any file in the space is served, to whoever may read it.
    let query = avatar
        .strip_prefix("https://host.test/xrpc/fyi.opensocial.internal.getSpaceBlob?")
        .unwrap();
    let lxm = format!("{INTERNAL}.getSpaceBlob");
    let (k, v) = auth(&h, &alice, &lxm);
    let res = h
        .http
        .get(format!("{}/xrpc/{lxm}?{query}", h.url))
        .header(k, v)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.bytes().await.unwrap().as_ref(), b"face");
}

#[tokio::test]
async fn refuses_a_request_that_has_expired_or_never_was() {
    let h = start(&[], false).await;
    let (riders, _, [alice, ..]) = groups(&h).await;
    for uri in [
        "urn:ietf:params:oauth:request_uri:req-nothing",
        "req-nothing",
    ] {
        assert_eq!(
            error(&offer(&h, &alice, uri).await),
            (400, "ExpiredRequest")
        );
        let said = call(
            &h,
            &alice,
            "approveAuthorization",
            json!({"requestUri": uri, "group": riders}),
        )
        .await;
        assert_eq!(error(&said), (400, "ExpiredRequest"));
        let said = call(
            &h,
            &alice,
            "declineAuthorization",
            json!({"requestUri": uri}),
        )
        .await;
        assert_eq!(error(&said), (400, "ExpiredRequest"));
    }
    let lxm = format!("{INTERNAL}.getAuthorization");
    assert_eq!(get(&h, &lxm, "", &[auth(&h, &alice, &lxm)]).await.0, 400);
    assert_eq!(get(&h, &lxm, "requestUri=x", &[]).await.0, 401);
    for method in ["approveAuthorization", "declineAuthorization"] {
        assert_eq!(
            error(&call(&h, &alice, method, json!({})).await),
            (400, "InvalidRequest")
        );
    }
}

#[tokio::test]
async fn approves_only_for_a_member_of_the_group_the_app_asked_for() {
    let h = start(&[], false).await;
    let (riders, hikers, [alice, bob, carol]) = groups(&h).await;
    let app = App::loopback(SCOPE);
    let request_uri = app.push(&h, &[]).await;
    let approve = async |who: &Authority, group: &str| {
        call(
            &h,
            who,
            "approveAuthorization",
            json!({"requestUri": request_uri, "group": group}),
        )
        .await
    };
    assert_eq!(error(&approve(&carol, &riders).await), (403, "Forbidden"));
    assert_eq!(
        error(&approve(&alice, "did:plc:nothing").await),
        (400, "GroupNotFound")
    );
    let said = call(
        &h,
        &alice,
        "approveAuthorization",
        json!({"requestUri": request_uri}),
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    call(&h, &alice, "deactivateGroup", json!({"group": hikers})).await;
    assert_eq!(approve(&alice, &hikers).await.0, 400);

    // Bob gets what his roles allow, and the grant is his.
    let (status, approved) = approve(&bob, &riders).await;
    assert_eq!(status, 200, "{approved}");
    let tokens = app
        .exchange(&h, approved["redirectUrl"].as_str().unwrap())
        .await;
    assert_eq!(tokens.body["scope"], "atproto");
    // Approved once, it waits no longer.
    assert_eq!(
        error(&approve(&alice, &riders).await),
        (400, "ExpiredRequest")
    );

    let named = app.push(&h, &[("login_hint", RIDERS)]).await;
    let said = call(
        &h,
        &alice,
        "approveAuthorization",
        json!({"requestUri": named, "group": hikers}),
    )
    .await;
    assert_eq!(error(&said), (400, "RepoDeactivated"));
    call(&h, &alice, "activateGroup", json!({"group": hikers})).await;
    let said = call(
        &h,
        &alice,
        "approveAuthorization",
        json!({"requestUri": named, "group": hikers}),
    )
    .await;
    assert_eq!(error(&said), (403, "Forbidden"));
}

#[tokio::test]
async fn gives_each_permission_set_as_the_scopes_it_lists() {
    let h = start(&[], false).await;
    let (riders, _, [alice, ..]) = groups(&h).await;
    let tokens = App::loopback(SCOPE).sign_in(&h, &alice, &riders).await;
    let scope = tokens["scope"].as_str().unwrap();
    assert!(!scope.contains("include:"), "{scope}");
    assert!(scope.contains(&format!("space:fyi.opensocial.meta?authority={riders}&skey=self&collection=fyi.opensocial.profile")), "{scope}");
    assert!(
        scope.contains("rpc:fyi.opensocial.listModSubjects?aud=*"),
        "{scope}"
    );
}

#[tokio::test]
async fn declines_with_access_denied_and_nothing_to_exchange() {
    let h = start(&[], false).await;
    let (_, _, [alice, ..]) = groups(&h).await;
    let app = App::loopback(SCOPE);
    let request_uri = app.push(&h, &[]).await;
    let (status, declined) = call(
        &h,
        &alice,
        "declineAuthorization",
        json!({"requestUri": request_uri}),
    )
    .await;
    assert_eq!(status, 200, "{declined}");
    let to = declined["redirectUrl"].as_str().unwrap();
    assert_eq!(param(to, "error").unwrap(), "access_denied");
    assert_eq!(param(to, "state").unwrap(), "st4te");
    assert_eq!(param(to, "iss").unwrap(), "https://host.test");
    assert_eq!(param(to, "code"), None);
    let said = call(
        &h,
        &alice,
        "declineAuthorization",
        json!({"requestUri": request_uri}),
    )
    .await;
    assert_eq!(error(&said), (400, "ExpiredRequest"));
}
