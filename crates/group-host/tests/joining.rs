//! Joining: at once, by request, or not without an invite; requests
//! listed, admitted, turned down and withdrawn.

mod common;

use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use serde_json::{Value as Json, json};

const NS: &str = "fyi.opensocial";

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
    // Its default roles, or member when none is marked.
    assert_eq!(
        call(&h, &bob, "requestJoin", json!({"group": did})).await,
        (200, json!({"status": "admitted"}))
    );
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
    call(
        &h,
        &alice,
        "putRole",
        json!({"group": did, "role": "rider", "name": "Rider", "actions": [], "default": true}),
    )
    .await;
    let carol = authority(&h, false, true).await;
    call(&h, &carol, "requestJoin", json!({"group": did})).await;
    assert_eq!(h.host.roles(&did, &carol.did).unwrap(), ["member", "rider"]);
    // With none marked, member.
    for role in ["member", "rider"] {
        call(
            &h,
            &alice,
            "putRole",
            json!({"group": did, "role": role, "name": role, "actions": [], "default": false}),
        )
        .await;
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
        (400, "InviteRequired")
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
    assert_eq!(asked, (200, json!({"status": "pending"})));
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
            mine["request"]["did"].clone(),
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
    assert_eq!(theirs["group"]["viewer"]["request"]["did"], bob.did);
    let (_, admins) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    let waiting: Vec<&str> = admins["joinRequests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["actor"]["did"].as_str().unwrap())
        .collect();
    assert_eq!(waiting, [carol.did.as_str(), bob.did.as_str()]);

    // Listed for one who may admit, newest first, a page at a time.
    let (_, page) = ask(
        &h,
        &alice,
        "listJoinRequests",
        &format!("group={did}&limit=1"),
    )
    .await;
    assert_eq!(page["requests"][0]["did"], carol.did);
    let cursor = page["cursor"].as_str().unwrap();
    let (_, rest) = ask(
        &h,
        &alice,
        "listJoinRequests",
        &format!("group={did}&cursor={cursor}"),
    )
    .await;
    assert_eq!(
        (rest["requests"][0]["did"].clone(), rest["cursor"].clone()),
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
        (400, "InvalidRequest")
    );

    // Admitted with the defaults, or the roles named that the admitter may give.
    let said = call(
        &h,
        &alice,
        "admitMember",
        json!({"group": did, "did": bob.did, "roles": [{"role": "admin"}]}),
    )
    .await;
    assert_eq!(error(&said), (400, "NotAssignable"));
    assert_eq!(
        call(
            &h,
            &alice,
            "admitMember",
            json!({"group": did, "did": bob.did})
        )
        .await,
        (200, json!({}))
    );
    assert_eq!(h.host.roles(&did, &bob.did).unwrap(), ["member"]);
    assert_eq!(
        ask(&h, &bob, "getJoinRequest", &format!("group={did}"))
            .await
            .1,
        json!({})
    );
    assert_eq!(
        error(
            &call(
                &h,
                &alice,
                "admitMember",
                json!({"group": did, "did": bob.did})
            )
            .await
        ),
        (400, "JoinRequestNotFound")
    );
    // A request gone stale, its asker a member another way: refused,
    // and gone.
    let eve = authority(&h, false, true).await;
    call(&h, &eve, "requestJoin", json!({"group": did})).await;
    h.host
        .space_write(
            &did,
            &group_host::group::members_space(&did),
            &[group_host::repo::Write::Create {
                collection: format!("{NS}.membership"),
                rkey: eve.did.clone(),
                value: json!({"$type": format!("{NS}.membership"), "member": eve.did, "roles": [{"role": "member"}], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    let said = call(
        &h,
        &alice,
        "admitMember",
        json!({"group": did, "did": eve.did}),
    )
    .await;
    assert_eq!(error(&said), (400, "AlreadyMember"));
    assert_eq!(
        ask(&h, &eve, "getJoinRequest", &format!("group={did}"))
            .await
            .1,
        json!({})
    );
    // Nobody is admitted with no roles.
    let said = call(
        &h,
        &alice,
        "admitMember",
        json!({"group": did, "did": carol.did, "roles": []}),
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    // A message is as long as the lexicon says.
    let frank = authority(&h, false, true).await;
    let long = "x".repeat(301);
    let said = call(
        &h,
        &frank,
        "requestJoin",
        json!({"group": did, "message": long}),
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidRequest"));

    // A member who may not admit, may not.
    let said = call(
        &h,
        &bob,
        "admitMember",
        json!({"group": did, "did": carol.did}),
    )
    .await;
    assert_eq!(error(&said), (403, "Forbidden"));
    let said = call(
        &h,
        &alice,
        "admitMember",
        json!({"group": did, "did": carol.did, "roles": [{"role": "member"}]}),
    )
    .await;
    assert_eq!(said.0, 200);

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
        (&alice, "admitMember", json!({"group": did})),
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
