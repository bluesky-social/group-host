//! A group's roles and who holds them: defined, given, taken away; people
//! put out, and leaving; and what they read no more once out.

mod common;

use atproto::space_auth::{self, Kind, Make};
use atproto::{Curve, Keypair};
use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::members_space;
use group_host::rbac::{grants, listed};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const NS: &str = "fyi.opensocial";

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// A group Alice started, Bob a plain member, Carol not one.
async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    join(h, &did, &bob.did, &["member"]).await;
    (did, [alice, bob, carol])
}

async fn join(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let roles: Vec<Json> = roles.iter().map(|r| json!({"role": r})).collect();
    h.host
        .space_write(
            did,
            &members_space(did),
            &[Write::Create {
                collection: format!("{NS}.membership"),
                rkey: who.into(),
                value: json!({"$type": format!("{NS}.membership"), "member": who, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
}

fn record(h: &Running, did: &str, collection: &str, rkey: &str) -> Option<Json> {
    h.host
        .group_record(
            did,
            &members_space(did),
            &format!("{NS}.{collection}"),
            rkey,
        )
        .unwrap()
}

fn grant_of(h: &Running, did: &str, role: &str) -> Option<group_host::rbac::Grant> {
    grants(&record(h, did, "permissions", "self").unwrap())
        .into_iter()
        .find(|g| g.role == role)
}

#[tokio::test]
async fn an_admin_defines_roles() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let (status, put) = call(
        &h,
        &alice,
        "putRole",
        json!({
            "group": did, "role": "moderator", "name": "Moderator", "description": "Keeps the peace.",
            "actions": ["label", "eject"], "assignable": [{"role": "member"}],
            "repoCollections": [{"collection": "fyi.opensocial.post"}], "bot": false, "default": false,
        }),
    )
    .await;
    assert_eq!(status, 200, "{put}");
    assert_eq!(
        put["uri"],
        format!("{}/{did}/{NS}.role/moderator", members_space(&did))
    );
    let role = record(&h, &did, "role", "moderator").unwrap();
    assert_eq!(
        (role["displayName"].clone(), role.get("default")),
        (json!("Moderator"), None)
    );
    let g = grant_of(&h, &did, "moderator").unwrap();
    assert_eq!(g.assignable, ["member"]);
    assert!(g.scopes.contains(&"repo:fyi.opensocial.post".to_string()));

    // A scope it held that is no action of the host's stays; so do its
    // collections when not named; whether it is given on joining too.
    let mut permissions = record(&h, &did, "permissions", "self").unwrap();
    for g in permissions["grants"].as_array_mut().unwrap() {
        if g["role"] == "moderator" {
            g["scopes"]
                .as_array_mut()
                .unwrap()
                .push(json!({"scope": "blob:image/*"}));
        }
    }
    call(&h, &alice, "putRole", json!({"group": did, "role": "moderator", "name": "Moderator", "actions": ["label"], "default": true})).await;
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[Write::Update {
                collection: format!("{NS}.permissions"),
                rkey: "self".into(),
                value: permissions,
            }],
        )
        .await
        .unwrap();
    let (status, _) = call(&h, &alice, "putRole", json!({"group": did, "role": "moderator", "name": "Mod", "actions": ["label"], "bot": true})).await;
    assert_eq!(status, 200);
    let g = grant_of(&h, &did, "moderator").unwrap();
    assert!(g.scopes.contains(&"blob:image/*".to_string()));
    assert!(g.scopes.contains(&"repo:fyi.opensocial.post".to_string()));
    let role = record(&h, &did, "role", "moderator").unwrap();
    assert_eq!(
        (role["default"].clone(), role["bot"].clone()),
        (json!(true), json!(true))
    );
    // Named, its collections are as named.
    call(&h, &alice, "putRole", json!({"group": did, "role": "moderator", "name": "Mod", "actions": [], "repoCollections": []})).await;
    let g = grant_of(&h, &did, "moderator").unwrap();
    assert!(!g.scopes.iter().any(|s| s.starts_with("repo:")));

    for (who, body, refused) in [
        (
            &bob,
            json!({"group": did, "role": "x", "name": "X", "actions": []}),
            (403, "Forbidden"),
        ),
        (
            &alice,
            json!({"group": did, "role": "admin", "name": "Admin", "actions": ["invite"]}),
            (400, "LastConfigurer"),
        ),
        (
            &alice,
            json!({"group": did, "role": "x", "name": "X", "actions": ["fly"]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "role": "a/b", "name": "X", "actions": []}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "role": "x", "actions": []}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "role": "x", "name": "X"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "role": "x", "name": "X", "actions": [], "bot": "yes"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"role": "x", "name": "X", "actions": []}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = call(&h, who, "putRole", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }
}

#[tokio::test]
async fn an_admin_takes_away_a_role_nobody_holds() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    call(
        &h,
        &alice,
        "putRole",
        json!({"group": did, "role": "helper", "name": "Helper", "actions": []}),
    )
    .await;
    assert_eq!(
        call(
            &h,
            &alice,
            "deleteRole",
            json!({"group": did, "role": "helper"})
        )
        .await,
        (200, json!({}))
    );
    assert_eq!(record(&h, &did, "role", "helper"), None);
    assert_eq!(grant_of(&h, &did, "helper"), None);
    // One with a grant and no record goes as well.
    assert_eq!(
        call(
            &h,
            &alice,
            "deleteRole",
            json!({"group": did, "role": "ghost"})
        )
        .await
        .0,
        200
    );
    for (who, body, refused) in [
        (
            &alice,
            json!({"group": did, "role": "member"}),
            (400, "RoleInUse"),
        ),
        (
            &alice,
            json!({"group": did, "role": "admin"}),
            (400, "LastConfigurer"),
        ),
        (
            &bob,
            json!({"group": did, "role": "helper"}),
            (403, "Forbidden"),
        ),
        (&alice, json!({"group": did}), (400, "InvalidRequest")),
    ] {
        let said = call(&h, who, "deleteRole", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }
}

#[tokio::test]
async fn roles_are_given_by_those_who_may() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    call(&h, &alice, "putRole", json!({"group": did, "role": "moderator", "name": "Moderator", "actions": ["eject"], "assignable": [{"role": "member"}]})).await;
    // An admin gives out only member, until it may give more.
    let give = |roles: Json| json!({"group": did, "member": bob.did, "roles": roles});
    let said = call(
        &h,
        &alice,
        "assignRoles",
        give(json!([{"role": "member"}, {"role": "moderator"}])),
    )
    .await;
    assert_eq!(error(&said), (400, "NotAssignable"));
    let every: Vec<&str> = group_host::rbac::ACTIONS.to_vec();
    call(&h, &alice, "putRole", json!({"group": did, "role": "admin", "name": "Admin", "actions": every, "assignable": [{"role": "member"}, {"role": "moderator"}]})).await;
    let said = call(
        &h,
        &alice,
        "assignRoles",
        give(json!([{"role": "member"}, {"role": "moderator"}])),
    )
    .await;
    assert_eq!(said, (200, json!({})));
    assert_eq!(
        h.host.roles(&did, &bob.did).unwrap(),
        ["member", "moderator"]
    );

    for (who, body, refused) in [
        (
            &alice,
            give(json!([{"role": "moderator"}])),
            (400, "MemberRole"),
        ),
        (
            &alice,
            json!({"group": did, "member": carol.did, "roles": [{"role": "member"}]}),
            (400, "NotAMember"),
        ),
        (
            &alice,
            give(json!([{"role": "member"}, {"role": "nobody"}])),
            (400, "InvalidRequest"),
        ),
        (&bob, give(json!([{"role": "member"}])), (403, "Forbidden")),
        (
            &alice,
            json!({"group": did, "member": bob.did}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "roles": []}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = call(&h, who, "assignRoles", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }
}

#[tokio::test]
async fn one_put_out_reads_no_more() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let members = members_space(&did);
    // Bob has a credential to the members space, and wrote there.
    let key = Keypair::generate(Curve::P256);
    let aud = format!("{did}#atproto_pds");
    let make = Make {
        iss: &bob.did,
        sub: &members,
        aud: Some(&aud),
        ..Make::default()
    };
    let token = space_auth::create(Kind::Delegation, &make, &bob.key, now()).unwrap();
    let body = json!({"space": members, "credentialKid": key.public().did_key(), "delegationToken": token});
    let (status, issued) = post(&h, "com.atproto.space.getSpaceCredential", &body, &[]).await;
    assert_eq!(status, 200, "{issued}");
    let (_, credential) =
        space_auth::parse(Kind::Credential, issued["credential"].as_str().unwrap()).unwrap();
    h.host
        .store
        .record_writer(
            &members,
            &bob.did,
            "3l5mz5mkbn22b",
            &[1; 32],
            &h.host.clock.next(),
        )
        .unwrap();
    h.host
        .store
        .index(
            &members,
            &bob.did,
            &group_host::store::SpaceHead {
                rev: "3l5mz5mkbn22b".into(),
                state: vec![0; 2048],
            },
            true,
            &Default::default(),
            0,
        )
        .unwrap();
    // And Carol writes there from elsewhere, so her host is told.
    join(&h, &did, &carol.did, &["member"]).await;
    h.host
        .store
        .record_writer(
            &members,
            &carol.did,
            "3l5mz5mkbn22b",
            &[2; 32],
            &h.host.clock.next(),
        )
        .unwrap();
    // A writer whose host is not found is told nothing, and holds nothing up.
    h.host
        .store
        .record_writer(
            &members,
            "did:plc:ghost",
            "3l5mz5mkbn22b",
            &[3; 32],
            &h.host.clock.next(),
        )
        .unwrap();

    // A moderator may put out only those it may.
    call(&h, &alice, "putRole", json!({"group": did, "role": "moderator", "name": "Moderator", "actions": ["eject"], "assignable": [{"role": "member"}]})).await;
    let dave = authority(&h, false, true).await;
    join(&h, &did, &dave.did, &["member", "moderator"]).await;
    let said = call(
        &h,
        &dave,
        "ejectMember",
        json!({"group": did, "member": alice.did}),
    )
    .await;
    assert_eq!(error(&said), (400, "NotAssignable"));

    assert_eq!(
        call(
            &h,
            &dave,
            "ejectMember",
            json!({"group": did, "member": bob.did})
        )
        .await,
        (200, json!({}))
    );
    assert_eq!(record(&h, &did, "membership", &bob.did), None);
    assert!(
        h.host
            .store
            .revoked(&members, &credential.jti, now() as i64)
            .unwrap()
    );
    assert_eq!(h.host.store.index_state(&members, &bob.did).unwrap(), None);
    let told = carol.told.writes.lock().unwrap().clone();
    assert_eq!(
        told[0].1,
        json!({"space": members, "credentials": [credential.jti]})
    );
    // And the index keeps him out.
    h.host.sync_space(&members).await.unwrap();
    assert_eq!(h.host.store.index_state(&members, &bob.did).unwrap(), None);

    for (who, body, refused) in [
        (
            &dave,
            json!({"group": did, "member": bob.did}),
            (400, "NotAMember"),
        ),
        (
            &carol,
            json!({"group": did, "member": dave.did}),
            (403, "Forbidden"),
        ),
        (&dave, json!({"group": did}), (400, "InvalidRequest")),
    ] {
        let said = call(&h, who, "ejectMember", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }
}

#[tokio::test]
async fn the_last_who_runs_it_is_neither_put_out_nor_stripped() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    // An admin who may give and take admin, as groups often set it.
    let every: Vec<&str> = group_host::rbac::ACTIONS.to_vec();
    call(&h, &alice, "putRole", json!({"group": did, "role": "admin", "name": "Admin", "actions": every, "assignable": [{"role": "member"}, {"role": "admin"}]})).await;
    let said = call(
        &h,
        &alice,
        "ejectMember",
        json!({"group": did, "member": alice.did}),
    )
    .await;
    assert_eq!(error(&said), (400, "LastConfigurer"));
    let said = call(
        &h,
        &alice,
        "assignRoles",
        json!({"group": did, "member": alice.did, "roles": [{"role": "member"}]}),
    )
    .await;
    assert_eq!(error(&said), (400, "LastConfigurer"));
    // With another admin, either may.
    call(
        &h,
        &alice,
        "assignRoles",
        json!({"group": did, "member": bob.did, "roles": [{"role": "member"}, {"role": "admin"}]}),
    )
    .await;
    let said = call(
        &h,
        &alice,
        "assignRoles",
        json!({"group": did, "member": alice.did, "roles": [{"role": "member"}]}),
    )
    .await;
    assert_eq!(said.0, 200);
    let said = call(
        &h,
        &bob,
        "ejectMember",
        json!({"group": did, "member": alice.did}),
    )
    .await;
    assert_eq!(said.0, 200);
}

#[tokio::test]
async fn members_leave_but_the_last_who_runs_it_stays() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    assert_eq!(
        call(&h, &bob, "leaveGroup", json!({"group": did})).await,
        (200, json!({}))
    );
    assert_eq!(record(&h, &did, "membership", &bob.did), None);
    assert_eq!(
        error(&call(&h, &bob, "leaveGroup", json!({"group": did})).await),
        (400, "NotAMember")
    );
    assert_eq!(
        error(&call(&h, &carol, "leaveGroup", json!({"group": did})).await),
        (400, "NotAMember")
    );
    assert_eq!(
        error(&call(&h, &alice, "leaveGroup", json!({"group": did})).await),
        (400, "LastConfigurer")
    );
    // With another to run it, she may.
    join(&h, &did, &carol.did, &["admin", "member"]).await;
    assert_eq!(
        call(&h, &alice, "leaveGroup", json!({"group": did}))
            .await
            .0,
        200
    );
    let roles = listed(
        &record(&h, &did, "membership", &carol.did).unwrap(),
        "roles",
        "role",
    );
    assert_eq!(roles, ["admin", "member"]);
    assert_eq!(
        error(&call(&h, &alice, "leaveGroup", json!({})).await),
        (400, "InvalidRequest")
    );
}

#[tokio::test]
async fn a_role_nobody_holds_keeps_nobody_running_it() {
    let h = start(&[], false).await;
    let (did, [alice, _, carol]) = group(&h).await;
    // A second role that configures, which nobody holds.
    let every: Vec<&str> = group_host::rbac::ACTIONS.to_vec();
    call(
        &h,
        &alice,
        "putRole",
        json!({"group": did, "role": "owner", "name": "Owner", "actions": every}),
    )
    .await;
    // Admin may not give it up then: nobody would be left who runs it.
    let said = call(
        &h,
        &alice,
        "putRole",
        json!({"group": did, "role": "admin", "name": "Admin", "actions": ["invite"]}),
    )
    .await;
    assert_eq!(error(&said), (400, "LastConfigurer"));
    assert!(grant_of(&h, &did, "admin").unwrap().scopes.len() > 1);
    // Once someone holds owner, it may.
    join(&h, &did, &carol.did, &["member", "owner"]).await;
    let said = call(
        &h,
        &alice,
        "putRole",
        json!({"group": did, "role": "admin", "name": "Admin", "actions": ["invite"]}),
    )
    .await;
    assert_eq!(said.0, 200);
}
