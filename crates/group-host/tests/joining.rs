//! Joining: at once, by request, or not without an invite; requests
//! listed, admitted by writing a membership as the group, turned down and
//! withdrawn.

mod common;

use common::groups::*;
use common::members::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::members_space;
use serde_json::{Value as Json, json};

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn ask(h: &Running, who: &Authority, method: &str, query: &str) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    get(h, &lxm, query, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

async fn group(h: &Running, founder: &Authority, policy: &str) -> String {
    start_group(
        h,
        founder,
        json!({"handle": RIDERS, "displayName": "Riders", "joinPolicy": policy}),
    )
    .await
}

#[tokio::test]
async fn an_open_group_admits_at_once() {
    let h = start(&[], false).await;
    let [alice, bob, _] = people(&h).await;
    let did = group(&h, &alice, "open").await;
    // Its default roles, or member when none is marked; the answer names
    // the membership made.
    let (status, said) = call(&h, &bob, "requestJoin", json!({"group": did})).await;
    assert_eq!((status, &said["status"]), (200, &json!("admitted")));
    let uri = format!("{}/{did}/{NS}.membership/{}", members_space(&did), bob.did);
    assert_eq!(said["membership"]["uri"], uri);
    assert_eq!(said["membership"]["cid"].as_str().map(str::len), Some(59));
    assert_eq!(said.as_object().unwrap().len(), 2);
    assert_eq!(h.host.roles(&did, &bob.did).unwrap(), ["member"]);
    assert_eq!(
        error(&call(&h, &bob, "requestJoin", json!({"group": did})).await),
        (400, "AlreadyMember")
    );
    // A request made before the group opened is gone once they are in.
    let eve = authority(&h, false, true).await;
    h.host.store.ask(&did, &eve.did, None, 0).unwrap();
    call(&h, &eve, "requestJoin", json!({"group": did})).await;
    assert_eq!(h.host.store.asked(&did, &eve.did).unwrap(), None);
    // Those marked default are what the next gets.
    let marked = |name: &str, default: bool| {
        let mut r = role(name);
        r["default"] = json!(default);
        r
    };
    keep(&h, &did, "role", "rider", marked("Rider", true)).await;
    let carol = authority(&h, false, true).await;
    call(&h, &carol, "requestJoin", json!({"group": did})).await;
    assert_eq!(h.host.roles(&did, &carol.did).unwrap(), ["member", "rider"]);
    // With member not marked, member all the same.
    keep(&h, &did, "role", "member", marked("Member", false)).await;
    let frank = authority(&h, false, true).await;
    call(&h, &frank, "requestJoin", json!({"group": did})).await;
    assert_eq!(h.host.roles(&did, &frank.did).unwrap(), ["member", "rider"]);
    // With none marked, member.
    for name in ["member", "rider"] {
        keep(&h, &did, "role", name, marked(name, false)).await;
    }
    let dave = authority(&h, false, true).await;
    call(&h, &dave, "requestJoin", json!({"group": did})).await;
    assert_eq!(h.host.roles(&did, &dave.did).unwrap(), ["member"]);
}

#[tokio::test]
async fn an_invite_only_group_needs_an_invite() {
    let h = start(&[], false).await;
    let [alice, bob, _] = people(&h).await;
    let did = group(&h, &alice, "invite").await;
    assert_eq!(
        error(&call(&h, &bob, "requestJoin", json!({"group": did})).await),
        (400, "JoinClosed")
    );
    for body in [json!({}), json!({"group": "did:plc:nobody"})] {
        assert_eq!(call(&h, &bob, "requestJoin", body).await.0, 400);
    }
    h.host.deactivate_account(&did, &alice.did, &[]).unwrap();
    assert_eq!(
        error(&call(&h, &bob, "requestJoin", json!({"group": did})).await),
        (400, "RepoDeactivated")
    );
}

#[tokio::test]
async fn a_group_that_approves_its_members_keeps_requests() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = group(&h, &alice, "approval").await;
    let asked = call(
        &h,
        &bob,
        "requestJoin",
        json!({"group": did, "message": "I ride too."}),
    )
    .await;
    assert_eq!(asked.0, 200);
    assert_eq!(asked.1["status"], "pending");
    assert_eq!(asked.1["request"]["member"], bob.did);
    assert_eq!(asked.1["request"]["message"], "I ride too.");
    assert!(asked.1["request"]["createdAt"].is_string());
    assert_eq!(asked.1.get("membership"), None);
    // Asked twice, still one.
    call(
        &h,
        &bob,
        "requestJoin",
        json!({"group": did, "message": "Again"}),
    )
    .await;
    call(&h, &carol, "requestJoin", json!({"group": did})).await;

    // Each sees their own.
    let (_, mine) = ask(&h, &bob, "getJoinRequest", &format!("group={did}")).await;
    assert_eq!(
        (
            mine["request"]["member"].clone(),
            mine["request"]["message"].clone()
        ),
        (json!(bob.did), json!("I ride too."))
    );
    let dave = authority(&h, false, true).await;
    assert_eq!(
        ask(&h, &dave, "getJoinRequest", &format!("group={did}")).await,
        (200, json!({}))
    );
    // And getGroup shows it to them, and all of them to whoever admits.
    let lxm = format!("{NS}.getGroup");
    let (_, theirs) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &bob, &lxm)]).await;
    assert_eq!(theirs["group"]["viewer"]["request"]["member"], bob.did);
    let (_, admins) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    let waiting: Vec<&str> = admins["joinRequests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["actor"]["did"].as_str().unwrap())
        .collect();
    assert_eq!(waiting, [carol.did.as_str(), bob.did.as_str()]);
    assert_eq!(
        admins["joinRequests"][0]["actor"]["handle"],
        "authority.test"
    );

    // Listed for one who may admit, newest first, a page at a time.
    let (_, page) = ask(
        &h,
        &alice,
        "listJoinRequests",
        &format!("group={did}&limit=1"),
    )
    .await;
    assert_eq!(page["requests"][0]["member"], carol.did);
    let cursor = page["cursor"].as_str().unwrap();
    let (_, rest) = ask(
        &h,
        &alice,
        "listJoinRequests",
        &format!("group={did}&cursor={cursor}"),
    )
    .await;
    assert_eq!(
        (
            rest["requests"][0]["member"].clone(),
            rest["cursor"].clone()
        ),
        (json!(bob.did), Json::Null)
    );
    assert_eq!(
        error(&ask(&h, &bob, "listJoinRequests", &format!("group={did}")).await),
        (403, "Forbidden")
    );
    assert_eq!(
        error(
            &ask(
                &h,
                &alice,
                "listJoinRequests",
                &format!("group={did}&cursor=x")
            )
            .await
        ),
        (400, "InvalidCursor")
    );

    // Writing their membership as the group admits them, with roles the
    // admitter's grant may give (an admin's, every role, those made
    // later too), and their request goes.
    let runs = Session::runs(&h, &alice, &did).await;
    keep(&h, &did, "role", "rider", role("Rider")).await;
    let said = runs
        .put(&h, "membership", &bob.did, membership(&["member", "rider"]))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(h.host.roles(&did, &bob.did).unwrap(), ["member", "rider"]);
    assert_eq!(
        ask(&h, &bob, "getJoinRequest", &format!("group={did}"))
            .await
            .1,
        json!({})
    );
    // One who never asked is not let in so.
    let said = runs
        .put(&h, "membership", &dave.did, membership(&["member"]))
        .await;
    assert_eq!(said.error(), (400, "JoinRequestNotFound"), "{}", said.body);
    // A request gone stale, its asker a member another way: written
    // again, their roles change, and the request goes.
    let eve = authority(&h, false, true).await;
    call(&h, &eve, "requestJoin", json!({"group": did})).await;
    keep(&h, &did, "membership", &eve.did, membership(&["member"])).await;
    let said = runs
        .put(&h, "membership", &eve.did, membership(&["member", "admin"]))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(
        ask(&h, &eve, "getJoinRequest", &format!("group={did}"))
            .await
            .1,
        json!({})
    );
    // Nobody is admitted without member, nor with a role the group does
    // not define.
    let said = runs
        .put(&h, "membership", &carol.did, membership(&[]))
        .await;
    assert_eq!(said.error(), (400, "MemberRole"), "{}", said.body);
    let said = runs
        .put(
            &h,
            "membership",
            &carol.did,
            membership(&["member", "ghost"]),
        )
        .await;
    assert_eq!(said.error(), (400, "InvalidRequest"), "{}", said.body);
    // A message is as long as the lexicon says.
    let frank = authority(&h, false, true).await;
    let ask_with = |words: String| json!({"group": did, "message": words});
    let said = call(&h, &frank, "requestJoin", ask_with("ü".repeat(2001))).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    let said = call(&h, &frank, "requestJoin", ask_with("ü".repeat(2000))).await;
    assert_eq!(said.1["status"], "pending");

    // A member who may not admit, may not.
    let bobs = Session::runs(&h, &bob, &did).await;
    let said = bobs
        .put(&h, "membership", &carol.did, membership(&["member"]))
        .await;
    assert_eq!(said.error(), (403, "ScopeMissing"), "{}", said.body);
    let said = runs
        .create(&h, "membership", &carol.did, membership(&["member"]))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);

    // Turned down, as if never asked: they may ask again.
    call(&h, &dave, "requestJoin", json!({"group": did})).await;
    assert_eq!(
        call(
            &h,
            &alice,
            "denyJoinRequest",
            json!({"group": did, "member": dave.did})
        )
        .await,
        (200, json!({}))
    );
    assert_eq!(
        ask(&h, &dave, "getJoinRequest", &format!("group={did}"))
            .await
            .1,
        json!({})
    );
    assert_eq!(
        error(
            &call(
                &h,
                &alice,
                "denyJoinRequest",
                json!({"group": did, "member": dave.did})
            )
            .await
        ),
        (400, "JoinRequestNotFound")
    );
    // Or withdrawn by its asker.
    call(&h, &dave, "requestJoin", json!({"group": did})).await;
    assert_eq!(
        call(&h, &dave, "cancelJoinRequest", json!({"group": did})).await,
        (200, json!({}))
    );
    assert_eq!(
        error(&call(&h, &dave, "cancelJoinRequest", json!({"group": did})).await),
        (400, "JoinRequestNotFound")
    );

    for (who, method, body) in [
        (&alice, "denyJoinRequest", json!({"group": did})),
        (&dave, "cancelJoinRequest", json!({})),
    ] {
        assert_eq!(
            error(&call(&h, who, method, body).await),
            (400, "InvalidRequest"),
            "{method}"
        );
    }
    // A member's getGroup has no request of theirs.
    let (_, members) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &bob, &lxm)]).await;
    assert_eq!(members["group"]["viewer"].get("request"), None);
}

#[tokio::test]
async fn admitting_takes_admit_and_gives_what_it_may() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = group(&h, &alice, "approval").await;
    // A gatekeeper may admit, and give member and rider, and nothing else.
    keep(&h, &did, "role", "rider", role("Rider")).await;
    let admit = group_host::rbac::scope_of("admit").unwrap();
    let gatekeeper = granting(&h, &did, "gatekeeper", &[&admit], &["member", "rider"]);
    keep(&h, &did, "role", "gatekeeper", gatekeeper).await;
    keep(
        &h,
        &did,
        "membership",
        &bob.did,
        membership(&["member", "gatekeeper"]),
    )
    .await;
    call(&h, &carol, "requestJoin", json!({"group": did})).await;
    let bobs = Session::of(&h, &bob, &did, &format!("atproto {admit}")).await;
    let said = bobs
        .put(
            &h,
            "membership",
            &carol.did,
            membership(&["member", "gatekeeper"]),
        )
        .await;
    assert_eq!(said.error(), (400, "NotAssignable"), "{}", said.body);
    let said = bobs
        .put(
            &h,
            "membership",
            &carol.did,
            membership(&["member", "rider"]),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(h.host.roles(&did, &carol.did).unwrap(), ["member", "rider"]);
    // Admitting is not giving roles to one who is in.
    let said = bobs
        .put(&h, "membership", &carol.did, membership(&["member"]))
        .await;
    assert_eq!(said.error(), (403, "ScopeMissing"), "{}", said.body);
}
