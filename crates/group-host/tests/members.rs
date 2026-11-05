//! A group's roles and who holds them, written as the group: roles
//! defined and taken away, what each may do, roles given; people put
//! out, and leaving; and what they read no more once out.

mod common;

use atproto::space_auth::{self, Kind, Make};
use atproto::{Curve, Keypair};
use common::groups::*;
use common::members::*;
use common::oauth::Said;
use common::spaces::*;
use common::{Running, start};
use group_host::group::members_space;
use group_host::rbac::{grants, roles_of, scope_of};
use group_host::scopes::covers;
use serde_json::{Value as Json, json};

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

fn refused(said: &Said, refusal: (u16, &str)) {
    assert_eq!(said.error(), refusal, "{}", said.body);
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
    keep(h, did, "membership", who, membership(who, roles)).await;
}

fn grant_of(h: &Running, did: &str, role: &str) -> Option<group_host::rbac::Grant> {
    grants(&record(h, did, "permissions", "self").unwrap())
        .into_iter()
        .find(|g| g.role == role)
}

/// The scope of each of `actions`.
fn scopes(actions: &[&str]) -> Vec<String> {
    actions.iter().map(|a| scope_of(a).unwrap()).collect()
}

#[tokio::test]
async fn an_admin_defines_roles_and_what_they_may_do() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    // A role and its grant, at once; the role is the admin's to give.
    let eject = scopes(&["label", "eject"]);
    let mut moderator = role("Moderator");
    moderator["description"] = json!("Keeps the peace.");
    let permissions = granting(
        &h,
        &did,
        "moderator",
        &[&eject[0], &eject[1], "repo:fyi.opensocial.post"],
        &["member"],
    );
    let permissions = {
        let mut p = permissions;
        for g in p["grants"].as_array_mut().unwrap() {
            if g["role"] == "admin" {
                g["assignable"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("moderator"));
            }
        }
        p
    };
    let said = runs
        .apply(
            &h,
            &[
                ("create", "role", "moderator", moderator),
                ("update", "permissions", "self", permissions),
            ],
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let kept = record(&h, &did, "role", "moderator").unwrap();
    assert_eq!(kept["displayName"], "Moderator");
    let g = grant_of(&h, &did, "moderator").unwrap();
    assert_eq!(g.assignable, ["member"]);
    assert!(g.scopes.contains(&"repo:fyi.opensocial.post".to_string()));
    assert_eq!(
        grant_of(&h, &did, "admin").unwrap().assignable,
        ["admin", "member", "moderator"]
    );
    // Given on joining, and its people bots, as the record says.
    let mut flagged = role("Mod");
    flagged["default"] = json!(true);
    flagged["bot"] = json!(true);
    assert_eq!(runs.put(&h, "role", "moderator", flagged).await.status, 200);
    let kept = record(&h, &did, "role", "moderator").unwrap();
    assert_eq!(
        (kept["default"].clone(), kept["bot"].clone()),
        (json!(true), json!(true))
    );
    // A scope given is one the giver's roles hold, as written or within
    // one they hold: a permission set of theirs is the scopes it lists.
    let rule = "space:fyi.opensocial.meta?skey=self&collection=fyi.opensocial.rule&action=create";
    let more = granting(
        &h,
        &did,
        "moderator",
        &[
            rule,
            "space:com.example.calendar?manage=create",
            "repo?collection=a.b.c&action=create",
        ],
        &["member"],
    );
    let said = runs.put(&h, "permissions", "self", more).await;
    assert_eq!(said.status, 200, "{}", said.body);

    let with = |role: &str, scopes: &[&str], assignable: &[&str]| {
        granting(&h, &did, role, scopes, assignable)
    };
    let every = scopes(&group_host::rbac::ACTIONS);
    let every: Vec<&str> = every.iter().map(String::as_str).collect();
    let cases = [
        // A key too long, or none a record may have.
        (
            runs.put(&h, "role", &"x".repeat(65), role("X")).await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(&h, "role", "a/b", role("X")).await,
            (400, "InvalidRequest"),
        ),
        // A record that is not what its lexicon says.
        (
            runs.put(&h, "role", "x", json!({"$type": c("role")})).await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(
                &h,
                "role",
                "x",
                json!({"$type": c("role"), "displayName": "x".repeat(65)}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(
                &h,
                "role",
                "x",
                json!({"$type": c("role"), "displayName": "X", "description": "x".repeat(2001)}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(
                &h,
                "role",
                "x",
                json!({"$type": c("role"), "displayName": "X", "bot": "yes"}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        // A grant of a role nobody may give, or one giving one.
        (
            runs.put(&h, "permissions", "self", with("ghost", &[], &[]))
                .await,
            (400, "NotAssignable"),
        ),
        (
            runs.put(
                &h,
                "permissions",
                "self",
                with("moderator", &[], &["moderator", "ghost"]),
            )
            .await,
            (400, "NotAssignable"),
        ),
        // Two grants of one role; a permissions record elsewhere, or none.
        (
            runs.put(&h, "permissions", "self", {
                let mut p = with("moderator", &[], &[]);
                let twice = p["grants"].as_array().unwrap().last().unwrap().clone();
                p["grants"].as_array_mut().unwrap().push(twice);
                p
            })
            .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(&h, "permissions", "other", with("moderator", &[], &[]))
                .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.delete(&h, "permissions", "self").await,
            (400, "InvalidRequest"),
        ),
        // A scope the admin does not hold.
        (
            runs.put(
                &h,
                "permissions",
                "self",
                with("moderator", &["blob:image/*"], &[]),
            )
            .await,
            (400, "ScopeNotHeld"),
        ),
        (
            runs.put(
                &h,
                "permissions",
                "self",
                with("moderator", &["include:fyi.opensocial.moderate"], &[]),
            )
            .await,
            (400, "ScopeNotHeld"),
        ),
        // Admin giving up running the group, with nobody else to.
        (
            runs.put(
                &h,
                "permissions",
                "self",
                with("admin", &[every[4]], &["admin", "member"]),
            )
            .await,
            (400, "LastConfigurer"),
        ),
    ];
    for (said, refusal) in cases {
        refused(&said, refusal);
    }
    // A role the admin may give, but the group does not define.
    let mut p = record(&h, &did, "permissions", "self").unwrap();
    gives(&mut p, "admin", "ghost");
    keep(&h, &did, "permissions", "self", p).await;
    for p in [with("ghost", &[], &[]), with("moderator", &[], &["ghost"])] {
        refused(
            &runs.put(&h, "permissions", "self", p).await,
            (400, "InvalidRequest"),
        );
    }
    // Nor anything at all, for a member whose roles may not.
    let bobs = Session::runs(&h, &bob, &did).await;
    refused(
        &bobs.put(&h, "role", "x", role("X")).await,
        (403, "ScopeMissing"),
    );
}

#[tokio::test]
async fn a_writer_changes_only_the_grants_of_roles_they_may_give() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    // A steward configures the group, and may give member only.
    let configure = scope_of("group.configure").unwrap();
    keep(&h, &did, "role", "steward", role("Steward")).await;
    let p = granting(&h, &did, "steward", &[&configure], &["member", "steward"]);
    keep(&h, &did, "permissions", "self", p).await;
    join(&h, &did, &bob.did, &["member", "steward"]).await;
    let bobs = Session::of(&h, &bob, &did, "atproto include:fyi.opensocial.configure").await;
    let with = |role: &str, scopes: &[&str], assignable: &[&str]| {
        granting(&h, &did, role, scopes, assignable)
    };
    // Not admin's grant; nor his own, to give admin, or what he lacks.
    let cases = [
        (
            bobs.put(
                &h,
                "permissions",
                "self",
                with("admin", &[], &["admin", "member"]),
            )
            .await,
            (400, "NotAssignable"),
        ),
        (
            bobs.put(
                &h,
                "permissions",
                "self",
                with("steward", &[&configure], &["member", "steward", "admin"]),
            )
            .await,
            (400, "NotAssignable"),
        ),
        (
            bobs.put(
                &h,
                "permissions",
                "self",
                with("steward", &[&configure, "repo:*"], &["member", "steward"]),
            )
            .await,
            (400, "ScopeNotHeld"),
        ),
    ];
    for (said, refusal) in cases {
        refused(&said, refusal);
    }
    // A role he makes in the same write is his to grant and to give.
    let mut p = with("helper", &[&configure], &[]);
    for g in p["grants"].as_array_mut().unwrap() {
        if g["role"] == "steward" {
            g["assignable"] = json!(["member", "steward", "helper"]);
        }
    }
    let said = bobs
        .apply(
            &h,
            &[
                ("create", "role", "helper", role("Helper")),
                ("update", "permissions", "self", p),
            ],
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(
        grant_of(&h, &did, "steward").unwrap().assignable,
        ["member", "steward", "helper"]
    );

    // Who signed the app in, and no longer holds the role, writes nothing.
    let alices = Session::runs(&h, &alice, &did).await;
    join(&h, &did, &bob.did, &["member"]).await;
    refused(
        &bobs.put(&h, "role", "x", role("X")).await,
        (403, "Forbidden"),
    );
    assert_eq!(alices.put(&h, "role", "x", role("X")).await.status, 200);
}

#[test]
fn reads_a_scope_as_covered_by_those_held() {
    let group = "did:plc:riders";
    let held: Vec<String> = [
        "repo:*",
        "space:com.example.calendar?skey=rides&collection=com.example.event",
        "space:*?manage=create",
        "rpc:com.example.thing?aud=did:web:example.com",
        "rpc:*?aud=*",
        "blob:image/*",
    ]
    .map(String::from)
    .to_vec();
    let some = |s: &str| covers(&held, s, group);
    for yes in [
        "repo:*",
        "repo:a.b.c?action=create",
        "repo?collection=a.b.c&collection=d.e.f",
        "space:com.example.calendar?skey=rides&collection=com.example.event&action=create",
        "space:com.example.calendar?skey=rides&action=read",
        "space?type=com.example.calendar&skey=rides&collection=com.example.event",
        "space:com.example.thing?manage=create",
        "rpc:com.example.other?aud=did:web:example.com",
        "blob:image/png",
        "blob?accept=image/jpeg",
    ] {
        assert!(some(yes), "{yes}");
    }
    for no in [
        "repo?action=create",
        "space:com.example.calendar?collection=com.example.event",
        "space:com.example.calendar?skey=rides&collection=com.example.other",
        "space:com.example.calendar?skey=rides&authority=*&collection=com.example.event",
        "space?skey=rides",
        "space:*?manage=delete",
        "blob:video/*",
        "blob?action=x",
        "transition:generic",
    ] {
        assert!(!some(no), "{no}");
    }
    let narrow = ["rpc:com.example.thing?aud=did:web:example.com".to_string()];
    assert!(covers(
        &narrow,
        "rpc:com.example.thing?aud=did:web:example.com",
        group
    ));
    assert!(!covers(&narrow, "rpc:com.example.thing?aud=*", group));
    assert!(!covers(
        &narrow,
        "rpc:com.example.other?aud=did:web:example.com",
        group
    ));
}

#[tokio::test]
async fn an_admin_takes_away_a_role_nobody_holds() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    keep(&h, &did, "role", "helper", role("Helper")).await;
    let mut p = granting(&h, &did, "helper", &[], &[]);
    gives(&mut p, "admin", "helper");
    keep(&h, &did, "permissions", "self", p.clone()).await;
    // Not while it has a grant, nor while someone holds it.
    refused(&runs.delete(&h, "role", "helper").await, (400, "RoleInUse"));
    refused(&runs.delete(&h, "role", "member").await, (400, "RoleInUse"));
    // The role and its grant, at once.
    let mut without = p;
    without["grants"]
        .as_array_mut()
        .unwrap()
        .retain(|g| g["role"] != "helper");
    let said = runs
        .apply(
            &h,
            &[
                ("delete", "role", "helper", Json::Null),
                ("update", "permissions", "self", without),
            ],
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(record(&h, &did, "role", "helper"), None);
    assert_eq!(grant_of(&h, &did, "helper"), None);
    // A grant that still lists it may change, as long as it lists it no
    // more than it did.
    let mut p = record(&h, &did, "permissions", "self").unwrap();
    for g in p["grants"].as_array_mut().unwrap() {
        if g["role"] == "admin" {
            g["scopes"]
                .as_array_mut()
                .unwrap()
                .retain(|s| s != "repo:*");
        }
    }
    let said = runs.put(&h, "permissions", "self", p).await;
    assert_eq!(said.status, 200, "{}", said.body);
}

#[tokio::test]
async fn roles_are_given_by_those_who_may() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    // A role the admin may not give, until a grant says so.
    keep(&h, &did, "role", "moderator", role("Moderator")).await;
    let eject = scope_of("eject").unwrap();
    let p = granting(&h, &did, "moderator", &[&eject], &["member"]);
    keep(&h, &did, "permissions", "self", p).await;
    let give = |roles: &[&str]| membership(&bob.did, roles);
    refused(
        &runs
            .put(&h, "membership", &bob.did, give(&["member", "moderator"]))
            .await,
        (400, "NotAssignable"),
    );
    let every = scopes(&group_host::rbac::ACTIONS);
    let every: Vec<&str> = every.iter().map(String::as_str).collect();
    let mut p = granting(&h, &did, "admin", &every, &["admin", "member", "moderator"]);
    p["grants"].as_array_mut().unwrap().last_mut().unwrap()["scopes"]
        .as_array_mut()
        .unwrap()
        .push(json!("repo:*"));
    keep(&h, &did, "permissions", "self", p).await;
    let said = runs
        .put(&h, "membership", &bob.did, give(&["member", "moderator"]))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(
        h.host.roles(&did, &bob.did).unwrap(),
        ["member", "moderator"]
    );

    let cases = [
        (
            runs.put(&h, "membership", &bob.did, give(&["moderator"]))
                .await,
            (400, "MemberRole"),
        ),
        (
            runs.put(&h, "membership", &bob.did, give(&["member", "nobody"]))
                .await,
            (400, "InvalidRequest"),
        ),
        // Kept at its member's DID.
        (
            runs.put(&h, "membership", &carol.did, give(&["member"]))
                .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(
                &h,
                "membership",
                &bob.did,
                json!({"$type": c("membership"), "member": bob.did}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        // One who never asked is not made a member.
        (
            runs.put(
                &h,
                "membership",
                &carol.did,
                membership(&carol.did, &["member"]),
            )
            .await,
            (400, "JoinRequestNotFound"),
        ),
    ];
    for (said, refusal) in cases {
        refused(&said, refusal);
    }
    let bobs = Session::runs(&h, &bob, &did).await;
    refused(
        &bobs
            .put(&h, "membership", &bob.did, give(&["member"]))
            .await,
        (403, "ScopeMissing"),
    );
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
    // Bob signed an app in as the group, and has an app password.
    let bobs = Session::of(&h, &bob, &did, "atproto").await;

    // A moderator may put out only those it may.
    keep(&h, &did, "role", "moderator", role("Moderator")).await;
    let eject = scope_of("eject").unwrap();
    let p = granting(&h, &did, "moderator", &[&eject], &["member"]);
    keep(&h, &did, "permissions", "self", p).await;
    let dave = authority(&h, false, true).await;
    join(&h, &did, &dave.did, &["member", "moderator"]).await;
    let daves = Session::of(&h, &dave, &did, &format!("atproto {eject}")).await;
    refused(
        &daves.delete(&h, "membership", &alice.did).await,
        (400, "NotAssignable"),
    );

    let said = daves.delete(&h, "membership", &bob.did).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(record(&h, &did, "membership", &bob.did), None);
    // The ejection stays in the group's queue, on him, by Dave.
    let events = h.host.store.events(&did, &bob.did, None, 0, 100).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "eject");
    assert_eq!(events[0].actor.as_deref(), Some(dave.did.as_str()));
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
    // The app he signed in acts as the group no more.
    let session = bobs
        .app
        .call(&h, "com.atproto.server.getSession", &bobs.token, None)
        .await;
    assert_eq!(session.status, 401, "{}", session.body);
    // And the index keeps him out.
    h.host.sync_space(&members).await.unwrap();
    assert_eq!(h.host.store.index_state(&members, &bob.did).unwrap(), None);

    // Put out once is out: there is nothing to delete again.
    let again = daves.delete(&h, "membership", &bob.did).await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(
        h.host
            .store
            .events(&did, &bob.did, None, 0, 100)
            .unwrap()
            .len(),
        1
    );
    let carols = Session::runs(&h, &carol, &did).await;
    refused(
        &carols.delete(&h, "membership", &dave.did).await,
        (403, "ScopeMissing"),
    );
}

#[tokio::test]
async fn the_last_who_runs_it_is_neither_put_out_nor_stripped() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    refused(
        &runs.delete(&h, "membership", &alice.did).await,
        (400, "LastConfigurer"),
    );
    let plain = membership(&alice.did, &["member"]);
    refused(
        &runs.put(&h, "membership", &alice.did, plain.clone()).await,
        (400, "LastConfigurer"),
    );
    // With another admin, either may.
    let said = runs
        .put(
            &h,
            "membership",
            &bob.did,
            membership(&bob.did, &["member", "admin"]),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(
        runs.put(&h, "membership", &alice.did, plain).await.status,
        200
    );
    let bobs = Session::runs(&h, &bob, &did).await;
    assert_eq!(bobs.delete(&h, "membership", &alice.did).await.status, 200);
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
    let roles = roles_of(&record(&h, &did, "membership", &carol.did).unwrap());
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
    let runs = Session::runs(&h, &alice, &did).await;
    // A second role that configures, which nobody holds.
    let configure = scope_of("group.configure").unwrap();
    let mut p = granting(&h, &did, "owner", &[&configure], &[]);
    for g in p["grants"].as_array_mut().unwrap() {
        if g["role"] == "admin" {
            g["assignable"].as_array_mut().unwrap().push(json!("owner"));
        }
    }
    let said = runs
        .apply(
            &h,
            &[
                ("create", "role", "owner", role("Owner")),
                ("update", "permissions", "self", p),
            ],
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Admin may not give it up then: nobody would be left who runs it.
    let invite = scope_of("invite").unwrap();
    let gives_up = granting(&h, &did, "admin", &[&invite], &["admin", "member", "owner"]);
    refused(
        &runs.put(&h, "permissions", "self", gives_up.clone()).await,
        (400, "LastConfigurer"),
    );
    assert!(grant_of(&h, &did, "admin").unwrap().scopes.len() > 1);
    // Once someone holds owner, it may.
    join(&h, &did, &carol.did, &["member", "owner"]).await;
    let said = runs.put(&h, "permissions", "self", gives_up).await;
    assert_eq!(said.status, 200, "{}", said.body);
}

#[tokio::test]
async fn one_whose_credentials_cannot_be_revoked_stays_in() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    // Revoking fails: the delete is not made, so it may be sent again.
    common::oauth::age(&h, "DROP TABLE issued_credential");
    let said = runs.delete(&h, "membership", &bob.did).await;
    assert_eq!(said.status, 500, "{}", said.body);
    assert!(record(&h, &did, "membership", &bob.did).is_some());
    assert!(
        h.host
            .store
            .events(&did, &bob.did, None, 0, 100)
            .unwrap()
            .is_empty()
    );
}

/// What `listMembers` answers `who` for `query`, past `group=`.
async fn listed(h: &Running, who: Option<&Authority>, group: &str, query: &str) -> (u16, Json) {
    let lxm = format!("{NS}.listMembers");
    let headers: Vec<_> = who.map(|w| auth(h, w, &lxm)).into_iter().collect();
    get(h, &lxm, &format!("group={group}{query}"), &headers).await
}

/// Each member a page names, by their DID.
fn dids(page: &Json) -> Vec<String> {
    let members = page["members"].as_array().unwrap().iter();
    members
        .map(|m| m["actor"]["did"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn members_are_listed_a_page_at_a_time() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    // Seven more, whom no DID document names.
    let nobodies: Vec<String> = ('a'..='g')
        .map(|c| format!("did:plc:nobodynobodynobody{c}"))
        .collect();
    for n in &nobodies {
        join(&h, &did, n, &["member"]).await;
    }

    // getGroup counts them, and lists none.
    let shown = view(&h, &did, Some(&bob)).await;
    assert_eq!(shown["group"]["memberCount"], 9);
    assert_eq!(shown.get("members"), None);

    // All of them, in pages of four, each once, in the order of their DIDs.
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = String::new();
    let mut pages = 0;
    loop {
        let (status, page) = listed(&h, Some(&bob), &did, &format!("&limit=4{cursor}")).await;
        assert_eq!(status, 200, "{page}");
        pages += 1;
        seen.extend(dids(&page));
        let Some(next) = page["cursor"].as_str() else {
            break;
        };
        cursor = format!("&cursor={next}");
    }
    assert_eq!(pages, 3);
    let mut every: Vec<String> = nobodies.clone();
    every.extend([alice.did.clone(), bob.did.clone()]);
    every.sort();
    assert_eq!(seen, every);
    // A page holding the last of them says there are no more.
    let (_, whole) = listed(&h, Some(&bob), &did, "&limit=9").await;
    assert_eq!(dids(&whole).len(), 9);
    assert_eq!(whole.get("cursor"), None);

    // Each with their roles, and named as their DID document names them.
    let (_, all) = listed(&h, Some(&alice), &did, "").await;
    let of = |who: &str| {
        let members = all["members"].as_array().unwrap();
        members
            .iter()
            .find(|m| m["actor"]["did"] == who)
            .unwrap()
            .clone()
    };
    assert_eq!(
        of(&alice.did)["roles"],
        json!([{"role": "admin"}, {"role": "member"}])
    );
    assert_eq!(of(&alice.did)["actor"]["handle"], "authority.test");
    assert_eq!(
        of(&nobodies[0]),
        json!({"actor": {"did": nobodies[0]}, "roles": [{"role": "member"}]})
    );

    // Narrowed to those whose handle or DID holds `q`, whatever its case.
    let (_, named) = listed(&h, Some(&bob), &did, "&q=AUTHORITY").await;
    let mut both = vec![alice.did.clone(), bob.did.clone()];
    both.sort();
    assert_eq!(dids(&named), both);
    let (_, one) = listed(&h, Some(&bob), &did, "&q=nobodyc").await;
    assert_eq!(dids(&one), [nobodies[2].clone()]);
    let (_, none) = listed(&h, Some(&bob), &did, "&q=nobody.else").await;
    assert_eq!(none, json!({"members": []}));
    // And a page at a time still.
    let (_, first) = listed(&h, Some(&bob), &did, "&q=nobody&limit=5").await;
    assert_eq!(dids(&first), nobodies[..5]);
    let next = first["cursor"].as_str().unwrap();
    let rest = format!("&q=nobody&limit=5&cursor={next}");
    let (_, rest) = listed(&h, Some(&bob), &did, &rest).await;
    assert_eq!(dids(&rest), nobodies[5..]);
    assert_eq!(rest.get("cursor"), None);

    // Only to whoever may read its members space.
    assert_eq!(
        error(&listed(&h, Some(&carol), &did, "").await),
        (403, "Forbidden")
    );
    assert_eq!(listed(&h, None, &did, "").await.0, 401);
    let lost = listed(&h, Some(&bob), "did:plc:nobody", "").await;
    assert_eq!(error(&lost), (400, "GroupNotFound"));
    // Nor does one not in it learn how many are.
    let theirs = view(&h, &did, Some(&carol)).await;
    assert_eq!(theirs["group"].get("memberCount"), None);
}
