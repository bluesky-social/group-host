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
use group_host::rbac::{Grant, roles_of, scope_of};
use group_host::scopes::covers;
use serde_json::{Value as Json, json};

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = nsid(method);
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
    keep(h, did, "membership", who, membership(roles)).await;
}

fn grant_of(h: &Running, did: &str, role: &str) -> Option<Grant> {
    record(h, did, "role", role).map(|r| Grant::of(role, &r))
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
    // A role and its grant, in one record; the admin gives every role.
    let eject = scopes(&["label", "eject"]);
    let mut moderator = granting(
        &h,
        &did,
        "moderator",
        &[&eject[0], &eject[1], "repo:org.atgrp.post"],
        &["member"],
    );
    moderator["description"] = json!("Keeps the peace.");
    let said = runs.create(&h, "role", "moderator", moderator).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let kept = record(&h, &did, "role", "moderator").unwrap();
    assert_eq!(kept["displayName"], "moderator");
    let g = grant_of(&h, &did, "moderator").unwrap();
    assert_eq!(g.can_assign, ["member"]);
    assert!(g.permissions.contains(&"repo:org.atgrp.post".to_string()));
    assert_eq!(grant_of(&h, &did, "admin").unwrap().can_assign, ["*"]);
    // Given on joining, and its people bots, as the record says.
    let mut flagged = granting(&h, &did, "moderator", &[&eject[1]], &["member"]);
    flagged["default"] = json!(true);
    flagged["bot"] = json!(true);
    assert_eq!(runs.put(&h, "role", "moderator", flagged).await.status, 200);
    let kept = record(&h, &did, "role", "moderator").unwrap();
    assert_eq!(
        (kept["default"].clone(), kept["bot"].clone()),
        (json!(true), json!(true))
    );
    // A permission given is one the giver's roles hold, as written or
    // within one they hold: a permission set of theirs is the permissions
    // it lists.
    let rule = "space:org.atgrp.members?skey=self&collection=org.atgrp.rule&action=create";
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
    let said = runs.put(&h, "role", "moderator", more).await;
    assert_eq!(said.status, 200, "{}", said.body);

    let with = |role: &str, permissions: &[&str], can_assign: &[&str]| {
        granting(&h, &did, role, permissions, can_assign)
    };
    let every = scopes(&group_host::rbac::ACTIONS);
    let every: Vec<&str> = every.iter().map(String::as_str).collect();
    let many: Vec<String> = (0..101).map(|i| format!("repo:a.b.c{i}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
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
            runs.put(
                &h,
                "role",
                "spare",
                json!({"$type": c("role"), "displayName": "x".repeat(65)}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(
                &h,
                "role",
                "spare",
                json!({"$type": c("role"), "displayName": "X", "description": "x".repeat(2001)}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(
                &h,
                "role",
                "spare",
                json!({"$type": c("role"), "displayName": "X", "bot": "yes"}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(&h, "role", "spare", with("spare", &many, &[]))
                .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(&h, "role", "spare", with("spare", &[""], &[]))
                .await,
            (400, "InvalidRequest"),
        ),
        // A role let give one the group does not define.
        (
            runs.put(&h, "role", "moderator", with("moderator", &[], &["ghost"]))
                .await,
            (400, "InvalidRequest"),
        ),
        // A scope the admin does not hold.
        (
            runs.put(
                &h,
                "role",
                "moderator",
                with("moderator", &["blob:image/*"], &[]),
            )
            .await,
            (400, "ScopeNotHeld"),
        ),
        (
            runs.put(
                &h,
                "role",
                "moderator",
                with("moderator", &["include:org.atgrp.authParticipate"], &[]),
            )
            .await,
            (400, "ScopeNotHeld"),
        ),
        // Admin giving up running the group, with nobody else to.
        (
            runs.put(&h, "role", "admin", with("admin", &[every[4]], &["*"]))
                .await,
            (400, "LastConfigurer"),
        ),
    ];
    for (said, refusal) in cases {
        refused(&said, refusal);
    }
    // Nor anything at all, for a member whose roles may not.
    let bobs = Session::runs(&h, &bob, &did).await;
    refused(
        &bobs.put(&h, "role", "spare", role("X")).await,
        (403, "ScopeMissing"),
    );
}

#[tokio::test]
async fn a_role_is_keyed_by_an_id_as_a_label_value_is_written() {
    let h = start(&[], false).await;
    let (did, [alice, _, _]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    // Lowercase letters and `-`, at least 3 of them and at most 64: apps
    // may show it. A role needs no name of its own; it is shown by its ID.
    let unnamed = json!({ "$type": c("role") });
    let longest = "x".repeat(64);
    for id in ["helpers", "leader", "ghost", "ride-lead", "abc", &longest] {
        let said = runs.put(&h, "role", id, unnamed.clone()).await;
        assert_eq!(said.status, 200, "{id}: {}", said.body);
        assert_eq!(record(&h, &did, "role", id).unwrap(), unnamed);
    }
    for id in ["ab", "x", "Helpers", "ride_lead", "ride.lead", "helper2"] {
        let said = runs.put(&h, "role", id, role("Odd")).await;
        refused(&said, (400, "InvalidRequest"));
        assert_eq!(
            said.body["message"],
            format!("a role ID is 3 to 64 lowercase letters and -, not {id}"),
            "{id}"
        );
        assert_eq!(record(&h, &did, "role", id), None);
    }
    // One kept under another key, as a group moved here may have it, may
    // still be taken away.
    keep(&h, &did, "role", "Old", role("Old")).await;
    let said = runs.delete(&h, "role", "Old").await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(record(&h, &did, "role", "Old"), None);
}

#[tokio::test]
async fn every_role_is_given_by_one_who_may_give_every_role() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    // A role made after the group: the admin's `*` covers it too.
    let said = runs.create(&h, "role", "later", role("Later")).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let said = runs
        .put(&h, "membership", &bob.did, membership(&["member", "later"]))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // She may let another role give every role, holding `*` herself.
    let deputy = granting(&h, &did, "deputy", &[], &["*"]);
    let said = runs.create(&h, "role", "deputy", deputy).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(grant_of(&h, &did, "deputy").unwrap().can_assign, ["*"]);
    // Without it, she gives only the roles listed, and never `*` again.
    let configure = scope_of("group.configure").unwrap();
    let assign = scope_of("role.assign").unwrap();
    let listed = granting(&h, &did, "admin", &[&configure, &assign], &["member"]);
    keep(&h, &did, "role", "admin", listed).await;
    let cases = [
        (
            runs.put(&h, "membership", &bob.did, membership(&["member"]))
                .await,
            (400, "NotAssignable"),
        ),
        (
            runs.put(
                &h,
                "role",
                "later",
                granting(&h, &did, "later", &[], &["*"]),
            )
            .await,
            (400, "NotAssignable"),
        ),
        (
            runs.create(&h, "role", "new", granting(&h, &did, "new", &[], &["*"]))
                .await,
            (400, "NotAssignable"),
        ),
    ];
    for (said, refusal) in cases {
        refused(&said, refusal);
    }
}

#[tokio::test]
async fn a_writer_changes_only_the_grants_of_roles_they_may_give() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    // A steward configures the group, and may give member and steward.
    let configure = scope_of("group.configure").unwrap();
    let steward = granting(&h, &did, "steward", &[&configure], &["member", "steward"]);
    keep(&h, &did, "role", "steward", steward).await;
    join(&h, &did, &bob.did, &["member", "steward"]).await;
    let bobs = Session::of(&h, &bob, &did, "atproto include:org.atgrp.authManageAdmin").await;
    let with = |role: &str, permissions: &[&str], can_assign: &[&str]| {
        granting(&h, &did, role, permissions, can_assign)
    };
    // Not admin's grant; nor his own, to give admin or every role, or
    // what he lacks; nor a new role's, beyond what he holds and gives.
    let mine = ["member", "steward"];
    let cases = [
        (
            bobs.put(&h, "role", "admin", with("admin", &[], &["*"]))
                .await,
            (400, "NotAssignable"),
        ),
        (
            bobs.put(
                &h,
                "role",
                "steward",
                with("steward", &[&configure], &["member", "steward", "admin"]),
            )
            .await,
            (400, "NotAssignable"),
        ),
        (
            bobs.put(
                &h,
                "role",
                "steward",
                with("steward", &[&configure], &["member", "steward", "*"]),
            )
            .await,
            (400, "NotAssignable"),
        ),
        (
            bobs.put(
                &h,
                "role",
                "steward",
                with("steward", &[&configure, "repo:*"], &mine),
            )
            .await,
            (400, "ScopeNotHeld"),
        ),
        (
            bobs.create(&h, "role", "helper", with("helper", &["repo:*"], &[]))
                .await,
            (400, "ScopeNotHeld"),
        ),
        (
            bobs.create(&h, "role", "helper", with("helper", &[], &["*"]))
                .await,
            (400, "NotAssignable"),
        ),
    ];
    for (said, refusal) in cases {
        refused(&said, refusal);
    }
    // A role he makes is his to grant, but not to give.
    let helper = with("helper", &[&configure], &["member"]);
    let said = bobs.create(&h, "role", "helper", helper).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(grant_of(&h, &did, "helper").unwrap().can_assign, ["member"]);
    let cases = [
        (
            bobs.put(
                &h,
                "role",
                "steward",
                with("steward", &[&configure], &["member", "steward", "helper"]),
            )
            .await,
            (400, "NotAssignable"),
        ),
        (
            bobs.put(&h, "role", "helper", with("helper", &[], &[]))
                .await,
            (400, "NotAssignable"),
        ),
        (
            bobs.delete(&h, "role", "helper").await,
            (400, "NotAssignable"),
        ),
    ];
    for (said, refusal) in cases {
        refused(&said, refusal);
    }
    // What changes no grant is his to change.
    let mut renamed = with("helper", &[&configure], &["member"]);
    renamed["displayName"] = json!("Helper");
    let said = bobs.put(&h, "role", "helper", renamed).await;
    assert_eq!(said.status, 200, "{}", said.body);

    // Who signed the app in, and no longer holds the role, writes nothing.
    let alices = Session::runs(&h, &alice, &did).await;
    join(&h, &did, &bob.did, &["member"]).await;
    refused(
        &bobs.put(&h, "role", "spare", role("X")).await,
        (403, "Forbidden"),
    );
    assert_eq!(alices.put(&h, "role", "spare", role("X")).await.status, 200);
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
        "space?spaceType=com.example.calendar&skey=rides&collection=com.example.event",
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
    let (did, [alice, bob, _]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    let label = scope_of("label").unwrap();
    keep(
        &h,
        &did,
        "role",
        "helper",
        granting(&h, &did, "helper", &[&label], &[]),
    )
    .await;
    let gives = granting(&h, &did, "giver", &[], &["helper"]);
    keep(&h, &did, "role", "giver", gives).await;
    join(&h, &did, &bob.did, &["member", "helper"]).await;
    // Not while someone holds it.
    refused(&runs.delete(&h, "role", "helper").await, (400, "RoleInUse"));
    refused(&runs.delete(&h, "role", "member").await, (400, "RoleInUse"));
    // Once nobody does, with its grant.
    join(&h, &did, &bob.did, &["member"]).await;
    let said = runs.delete(&h, "role", "helper").await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(record(&h, &did, "role", "helper"), None);
    // A role that still lists it may change, as long as it lists it no
    // more than it did.
    let said = runs
        .put(
            &h,
            "role",
            "giver",
            granting(&h, &did, "giver", &[&label], &["helper"]),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
}

#[tokio::test]
async fn roles_are_given_by_those_who_may() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let runs = Session::runs(&h, &alice, &did).await;
    // A role the admin may not give, while her grant lists others alone.
    let eject = scope_of("eject").unwrap();
    let moderator = granting(&h, &did, "moderator", &[&eject], &["member"]);
    keep(&h, &did, "role", "moderator", moderator).await;
    let every = scopes(&group_host::rbac::ACTIONS);
    let mut every: Vec<&str> = every.iter().map(String::as_str).collect();
    every.push("repo:*");
    let listed = granting(&h, &did, "admin", &every, &["admin", "member"]);
    keep(&h, &did, "role", "admin", listed).await;
    let give = |roles: &[&str]| membership(roles);
    refused(
        &runs
            .put(&h, "membership", &bob.did, give(&["member", "moderator"]))
            .await,
        (400, "NotAssignable"),
    );
    let admin = granting(&h, &did, "admin", &every, &["admin", "member", "moderator"]);
    keep(&h, &did, "role", "admin", admin).await;
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
            runs.put(&h, "membership", "not-a-did", give(&["member"]))
                .await,
            (400, "InvalidRequest"),
        ),
        (
            runs.put(
                &h,
                "membership",
                &bob.did,
                json!({"$type": c("membership")}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        // One who never asked is not made a member.
        (
            runs.put(&h, "membership", &carol.did, membership(&["member"]))
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
    let body = json!({"space": members, "credentialKey": key.public().did_key(), "delegationToken": token});
    let (status, issued) = post(&h, "com.atproto.space.authorizeSpaceSync", &body, &[]).await;
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
    let eject = scope_of("eject").unwrap();
    let moderator = granting(&h, &did, "moderator", &[&eject], &["member"]);
    keep(&h, &did, "role", "moderator", moderator).await;
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
    let events = h
        .host
        .store
        .events(&did, &bob.did, None, false, None, 100)
        .unwrap();
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
    // He leaves the space's writer set, the removal kept for its syncers.
    let log = h.host.store.writer_log(&members, None, 100).unwrap();
    let his = log.iter().find(|c| c.writer.did == bob.did).unwrap();
    assert!(his.removed);
    let writers = h.host.store.writers(&members, None, 100).unwrap();
    assert!(writers.iter().all(|w| w.did != bob.did));
    assert!(writers.iter().any(|w| w.did == carol.did));
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
            .events(&did, &bob.did, None, false, None, 100)
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
    // Said as a person reads it: no DIDs, method names or codes.
    let said = runs.delete(&h, "membership", &alice.did).await;
    refused(&said, (400, "LastConfigurer"));
    assert_eq!(
        said.body["message"],
        "That would leave the group with no admin. Make someone else an admin first."
    );
    let plain = membership(&["member"]);
    refused(
        &runs.put(&h, "membership", &alice.did, plain.clone()).await,
        (400, "LastConfigurer"),
    );
    // With another admin, either may.
    let said = runs
        .put(&h, "membership", &bob.did, membership(&["member", "admin"]))
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
    // Bob wrote in the members space.
    let members = members_space(&did);
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
    assert_eq!(
        call(&h, &bob, "leaveGroup", json!({"group": did})).await,
        (200, json!({}))
    );
    assert_eq!(record(&h, &did, "membership", &bob.did), None);
    // And leaves its writer set.
    let writers = h.host.store.writers(&members, None, 100).unwrap();
    assert!(writers.iter().all(|w| w.did != bob.did));
    let log = h.host.store.writer_log(&members, None, 100).unwrap();
    assert!(log.iter().any(|c| c.writer.did == bob.did && c.removed));
    assert_eq!(
        error(&call(&h, &bob, "leaveGroup", json!({"group": did})).await),
        (400, "NotAMember")
    );
    assert_eq!(
        error(&call(&h, &carol, "leaveGroup", json!({"group": did})).await),
        (400, "NotAMember")
    );
    let said = call(&h, &alice, "leaveGroup", json!({"group": did})).await;
    assert_eq!(error(&said), (400, "LastConfigurer"));
    assert_eq!(
        said.1["message"],
        "You're the group's only admin. Make someone else an admin first."
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
    let owner = granting(&h, &did, "owner", &[&configure], &[]);
    let said = runs.create(&h, "role", "owner", owner).await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Admin may not give it up then: nobody would be left who runs it.
    let invite = scope_of("invite").unwrap();
    let gives_up = granting(&h, &did, "admin", &[&invite], &["*"]);
    refused(
        &runs.put(&h, "role", "admin", gives_up.clone()).await,
        (400, "LastConfigurer"),
    );
    assert!(grant_of(&h, &did, "admin").unwrap().permissions.len() > 1);
    // Once someone holds owner, it may.
    join(&h, &did, &carol.did, &["member", "owner"]).await;
    let said = runs.put(&h, "role", "admin", gives_up).await;
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
            .events(&did, &bob.did, None, false, None, 100)
            .unwrap()
            .is_empty()
    );
}

/// What `listMembers` answers `who` for `query`, past `group=`.
async fn listed(h: &Running, who: Option<&Authority>, group: &str, query: &str) -> (u16, Json) {
    let lxm = format!("{INTERNAL}.listMembers");
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
