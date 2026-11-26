//! The spaces a group's members make beside its two own, through
//! atproto's space methods as the group: made, changed, read and ended by
//! those whose roles, or the space's own policy, allow it; each kept under
//! its policy, which the host publishes in the space and follows in the
//! group's index, and which decides who reads and who writes there.

mod common;

use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use group_host::authority::Access;
use group_host::group::{members_space, meta_space};
use group_host::repo::Write;
use group_host::store::Registration;
use serde_json::{Value as Json, json};

const CALENDAR: &str = "org.atgrp.calendar";
const POLICY: &str = "org.atgrp.policy";
const PUBLISHED: &str = "com.atproto.space.policy";

fn calendar(did: &str, skey: &str) -> String {
    format!("at://{did}/space/{CALENDAR}/{skey}")
}

fn policy(grants: Json) -> Json {
    json!({"$type": POLICY, "grants": grants})
}

/// `name`, one of atproto's space methods, as the group, by `who` with a
/// group token for `space`.
async fn method(
    h: &Running,
    who: &Authority,
    did: &str,
    space: &str,
    name: &str,
    body: Json,
) -> Said {
    let nsid = format!("com.atproto.space.{name}");
    as_group(h, who, did, space, &nsid, Some(&body)).await
}

/// `getSpace` for `space`, by `who` with a group token for it.
async fn get_space(h: &Running, who: &Authority, did: &str, space: &str) -> Said {
    let nsid = format!("com.atproto.space.getSpace?space={space}");
    as_group(h, who, did, space, &nsid, None).await
}

/// The policy as the host published it in `space`.
fn published(h: &Running, did: &str, space: &str) -> Option<Json> {
    h.host.group_record(did, space, PUBLISHED, "self").unwrap()
}

fn entry(h: &Running, did: &str, space: &str) -> Option<Json> {
    let entries = h
        .host
        .group_records(did, &members_space(did), &format!("{NS}.space"))
        .unwrap();
    entries
        .into_iter()
        .map(|(_, e)| e)
        .find(|e| e["space"] == space)
}

fn may(h: &Running, space: &str, who: Option<&str>, access: Access) -> bool {
    let hosted = h.host.store.space(space).unwrap().unwrap();
    h.host.may(space, &hosted, who, access).unwrap()
}

/// The members in `space`'s writer set, the group's own repo aside.
fn writers(h: &Running, space: &str) -> Vec<String> {
    let group = space.split('/').nth(2).unwrap();
    let writers = h.host.store.writers(space, None, 100).unwrap();
    let members = writers.into_iter().filter(|w| w.did != group);
    members.map(|w| w.did).collect()
}

/// `did`'s repo in `space`, in its writer set.
fn wrote(h: &Running, space: &str, did: &str) {
    let rev = h.host.clock.next();
    let kept = h.host.store.record_writer(space, did, &rev, &[7; 32], &rev);
    kept.unwrap();
}

/// Gives `who` `roles` in the group, as the host would.
async fn set_roles(h: &Running, did: &str, who: &Authority, roles: &[&str]) {
    let collection = format!("{NS}.membership");
    let members = members_space(did);
    let found = h.host.group_record(did, &members, &collection, &who.did);
    let mut value = found.unwrap().unwrap();
    value["roles"] = json!(roles);
    let update = Write::Update {
        collection,
        rkey: who.did.clone(),
        value,
    };
    h.host.space_write(did, &members, &[update]).await.unwrap();
}

/// A group Alice started, and Bob a plain member of it.
async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[Write::Create {
                collection: format!("{NS}.membership"),
                rkey: bob.did.clone(),
                value: json!({"$type": format!("{NS}.membership"), "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    (did, [alice, bob, carol])
}

#[tokio::test]
async fn an_admin_makes_a_space() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let rides = calendar(&did, "rides");
    let made = policy(json!([
        {"role": "member", "read": true},
        {"role": "admin", "read": true, "write": true, "permissions": [format!("space:{CALENDAR}?collection=org.atgrp.event")]},
    ]));
    let body = json!({"spaceType": CALENDAR, "skey": "rides", "policy": made});
    let said = method(&h, &alice, &did, &rides, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body, json!({ "space": rides }));
    // Kept under its policy, published in the space, and in the index.
    assert_eq!(h.host.store.space(&rides).unwrap().unwrap().policy, made);
    assert_eq!(
        published(&h, &did, &rides),
        Some(json!({"$type": PUBLISHED, "policy": made}))
    );
    assert!(entry(&h, &did, &rides).is_some());
    // Members read it; only admins write in it in their own name; nobody
    // else reads it.
    assert!(may(&h, &rides, Some(&bob.did), Access::Read));
    assert!(!may(&h, &rides, Some(&bob.did), Access::Write));
    assert!(may(&h, &rides, Some(&alice.did), Access::Write));
    assert!(!may(&h, &rides, Some(&carol.did), Access::Read));
    assert!(!may(&h, &rides, None, Access::Read));

    // publicRead, read by anyone, signed in or not; unlisted, in no index.
    let free = calendar(&did, "free");
    let open = json!({"$type": POLICY, "publicRead": true, "unlisted": true, "grants": []});
    let body = json!({"spaceType": CALENDAR, "skey": "free", "policy": open});
    let said = method(&h, &alice, &did, &free, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert!(may(&h, &free, None, Access::Read));
    assert!(may(&h, &free, Some(&carol.did), Access::Read));
    assert!(!may(&h, &free, Some(&carol.did), Access::Write));
    assert_eq!(entry(&h, &did, &free), None);
    // A token for one space makes no other.
    let body = json!({"spaceType": CALENDAR, "skey": "else", "policy": open});
    let said = method(&h, &alice, &did, &free, "createSpace", body).await;
    assert_eq!(said.error(), (403, "ScopeMissing"));
    // An app signed in to make calendars makes one with no key, given one
    // of its own.
    let app = App::loopback(&format!("atproto space:{CALENDAR}?manage=create"));
    let signed = app.sign_in(&h, &alice, &did).await;
    let token = signed["access_token"].as_str().unwrap();
    let body = json!({"spaceType": CALENDAR, "policy": policy(json!([]))});
    let said = app
        .call(&h, "com.atproto.space.createSpace", token, Some(&body))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let made = said.body["space"].as_str().unwrap();
    assert!(made.starts_with(&calendar(&did, "")));
    assert_ne!(made, rides);
    let app = App::new("console");

    let at = |skey: &str| calendar(&did, skey);
    for (space, body, refused) in [
        // Not a policy of this host's kind.
        (
            at("laps"),
            json!({"spaceType": CALENDAR, "skey": "laps", "policy": {"$type": "com.atproto.memberspace.policy", "publicRead": true}}),
            (400, "UnsupportedPolicy"),
        ),
        (
            at("rides"),
            json!({"spaceType": CALENDAR, "skey": "rides", "policy": policy(json!([]))}),
            (403, "ScopeMissing"),
        ),
        (
            at("laps"),
            json!({"spaceType": CALENDAR, "skey": "laps"}),
            (400, "InvalidRequest"),
        ),
        (
            at("laps"),
            json!({"spaceType": CALENDAR, "skey": "laps", "policy": {"$type": POLICY}}),
            (400, "InvalidRequest"),
        ),
        // A role the group has not, one twice, or a scope no space scope.
        (
            at("laps"),
            json!({"spaceType": CALENDAR, "skey": "laps", "policy": policy(json!([{"role": "guest", "read": true}]))}),
            (400, "InvalidRequest"),
        ),
        (
            at("laps"),
            json!({"spaceType": CALENDAR, "skey": "laps", "policy": policy(json!([{"role": "member"}, {"role": "member"}]))}),
            (400, "InvalidRequest"),
        ),
        (
            at("laps"),
            json!({"spaceType": CALENDAR, "skey": "laps", "policy": policy(json!([{"role": "member", "permissions": ["repo:org.atgrp.event"]}]))}),
            (400, "InvalidRequest"),
        ),
        // The meta and members spaces come with the group.
        (
            format!("at://{did}/space/{NS}.meta/other"),
            json!({"spaceType": format!("{NS}.meta"), "skey": "other", "policy": policy(json!([]))}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = method(&h, &alice, &did, &space, "createSpace", body.clone()).await;
        assert_eq!(said.error(), refused, "{body}");
    }

    // A policy that is no record (a float, which its lexicon lets through
    // and DRISL does not) leaves no space behind, and its key free.
    let laps = at("laps");
    let wrong = policy(json!([{"role": "member", "read": true, "weight": 1.5}]));
    let body = json!({"spaceType": CALENDAR, "skey": "laps", "policy": wrong});
    let said = method(&h, &alice, &did, &laps, "createSpace", body).await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
    assert!(h.host.store.space(&laps).unwrap().is_none());
    let body = json!({"spaceType": CALENDAR, "skey": "laps", "policy": policy(json!([]))});
    let said = method(&h, &alice, &did, &laps, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);

    // Bob's roles make no spaces: he has no token to, and one made while
    // he could is refused once he cannot.
    let said = method(&h, &bob, &did, &at("mine"), "createSpace", json!({})).await;
    assert_eq!(said.error(), (403, "Forbidden"));
    let token = app.token_for(&h, &alice, &did, &at("late")).await;
    set_roles(&h, &did, &alice, &["member"]).await;
    let body = json!({"spaceType": CALENDAR, "skey": "late", "policy": policy(json!([]))});
    let said = app
        .call(&h, "com.atproto.space.createSpace", &token, Some(&body))
        .await;
    assert_eq!(said.error(), (403, "NotAuthorized"));
}

#[tokio::test]
async fn a_space_changes_under_its_policy() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let rides = calendar(&did, "rides");
    let first = policy(json!([{"role": "member", "read": true, "write": true}]));
    let body = json!({"spaceType": CALENDAR, "skey": "rides", "policy": first});
    let said = method(&h, &alice, &did, &rides, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    wrote(&h, &rides, &bob.did);
    assert_eq!(writers(&h, &rides), std::slice::from_ref(&bob.did));

    // Members read it still but write no more, and leave its writer set;
    // it leaves the index, and its published policy follows.
    let next = json!({"$type": POLICY, "unlisted": true, "grants": [
        {"role": "member", "read": true, "permissions": [format!("space:{CALENDAR}?manage=update")]},
    ]});
    let body = json!({"space": rides, "policy": next});
    let said = method(&h, &alice, &did, &rides, "updateSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body, json!({}));
    assert_eq!(h.host.store.space(&rides).unwrap().unwrap().policy, next);
    assert_eq!(
        published(&h, &did, &rides),
        Some(json!({"$type": PUBLISHED, "policy": next}))
    );
    assert_eq!(entry(&h, &did, &rides), None);
    assert!(writers(&h, &rides).is_empty());
    assert!(!may(&h, &rides, Some(&bob.did), Access::Write));

    // getSpace answers it to a member who reads it.
    let said = get_space(&h, &bob, &did, &rides).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body, json!({"space": rides, "policy": next}));

    // The policy's own grant lets Bob change it, so long as he changes no
    // role's grant he may not assign: listed again, it is indexed again.
    let listed = json!({"$type": POLICY, "grants": next["grants"]});
    let body = json!({"space": rides, "policy": listed});
    let said = method(&h, &bob, &did, &rides, "updateSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert!(entry(&h, &did, &rides).is_some());
    let mine = policy(
        json!([{"role": "member", "read": true, "write": true, "permissions": [format!("space:{CALENDAR}?manage=update")]}]),
    );
    let body = json!({"space": rides, "policy": mine});
    let said = method(&h, &bob, &did, &rides, "updateSpace", body).await;
    assert_eq!(said.error(), (403, "NotAuthorized"));

    // Refusals, for Alice, who may change any space.
    let elsewhere = format!("at://did:plc:elsewhere/space/{CALENDAR}/rides");
    let none = calendar(&did, "none");
    for (space, body, refused) in [
        (
            rides.clone(),
            json!({"space": rides, "policy": {"$type": "com.atproto.personalspace.policy"}}),
            (400, "UnsupportedPolicy"),
        ),
        (
            rides.clone(),
            json!({"space": rides, "policy": policy(json!([{"role": "guest"}]))}),
            (400, "InvalidRequest"),
        ),
        (
            rides.clone(),
            json!({"space": rides}),
            (400, "InvalidRequest"),
        ),
        (
            rides.clone(),
            json!({"space": elsewhere, "policy": policy(json!([]))}),
            (400, "SpaceNotFound"),
        ),
        (
            rides.clone(),
            json!({"space": none, "policy": policy(json!([]))}),
            (400, "SpaceNotFound"),
        ),
        // A token for one space changes no other.
        (
            rides.clone(),
            json!({"space": members_space(&did), "policy": policy(json!([]))}),
            (403, "ScopeMissing"),
        ),
    ] {
        let said = method(&h, &alice, &did, &space, "updateSpace", body.clone()).await;
        assert_eq!(said.error(), refused, "{body}");
    }

    // Meta's publicRead is the group's visibility: its declaration follows.
    let declared = |h: &Running| {
        h.host
            .store
            .record(&did, &format!("{NS}.declaration"), "self")
            .unwrap()
            .is_some()
    };
    let meta = meta_space(&did);
    assert!(declared(&h));
    for (public, listed) in [(false, false), (false, false), (true, true), (true, true)] {
        let body = json!({"space": meta, "policy": {"$type": POLICY, "publicRead": public, "grants": [{"role": "member", "read": true}]}});
        let said = method(&h, &alice, &did, &meta, "updateSpace", body).await;
        assert_eq!(said.status, 200, "{}", said.body);
        assert_eq!(declared(&h), listed);
    }

    // Losing a role that writes takes its holder out of the writer set.
    let admins = calendar(&did, "admins");
    let only = policy(json!([{"role": "admin", "read": true, "write": true}]));
    let body = json!({"spaceType": CALENDAR, "skey": "admins", "policy": only});
    let said = method(&h, &alice, &did, &admins, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let members = members_space(&did);
    let app = App::new("console");
    let token = app.token_for(&h, &alice, &did, &members).await;
    let membership = |roles: Json| json!({"$type": format!("{NS}.membership"), "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let collection = format!("{NS}.membership");
    let said = app
        .put(
            &h,
            &token,
            &members,
            &collection,
            &bob.did,
            membership(json!(["admin", "member"])),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    wrote(&h, &admins, &bob.did);
    wrote(&h, &admins, &alice.did);
    let said = app
        .put(
            &h,
            &token,
            &members,
            &collection,
            &bob.did,
            membership(json!(["member"])),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(writers(&h, &admins), std::slice::from_ref(&alice.did));

    // getSpace: to a member who neither reads it nor may change it, no;
    // to a token for another space, no.
    let said = get_space(&h, &bob, &did, &admins).await;
    assert_eq!(said.error(), (403, "Forbidden"));
    let said = as_group(
        &h,
        &alice,
        &did,
        &rides,
        &format!("com.atproto.space.getSpace?space={admins}"),
        None,
    )
    .await;
    assert_eq!(said.error(), (403, "ScopeMissing"));
    // Nor for what is no space.
    let nsid = "com.atproto.space.getSpace?space=nowhere";
    let said = as_group(&h, &alice, &did, &rides, nsid, None).await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
    let token = app.token_for(&h, &bob, &did, &rides).await;
    set_roles(&h, &did, &bob, &[]).await;
    let nsid = format!("com.atproto.space.getSpace?space={rides}");
    let said = app.call(&h, &nsid, &token, None).await;
    assert_eq!(said.error(), (403, "NotAuthorized"));
    // Nor, his roles gone, may his token change it.
    let body = json!({"space": rides, "policy": next});
    let nsid = "com.atproto.space.updateSpace";
    let said = app.call(&h, nsid, &token, Some(&body)).await;
    assert_eq!(said.error(), (403, "NotAuthorized"));
}

#[tokio::test]
async fn the_host_alone_writes_a_policy_and_the_index() {
    let h = start(&[], false).await;
    let (did, [alice, _, _]) = group(&h).await;
    let (meta, members) = (meta_space(&did), members_space(&did));
    let app = App::new("console");
    let token = app.token_for(&h, &alice, &did, &meta).await;
    let said = app
        .put(
            &h,
            &token,
            &meta,
            PUBLISHED,
            "self",
            json!({"$type": PUBLISHED, "policy": policy(json!([]))}),
        )
        .await;
    assert_eq!(said.error(), (403, "ScopeMissing"));
    let token = app.token_for(&h, &alice, &did, &members).await;
    let index = format!("{NS}.space");
    let said = app
        .put(
            &h,
            &token,
            &members,
            &index,
            "3lzzzzzzzzzzz",
            json!({"$type": index, "space": meta, "createdAt": "2026-11-03T00:00:00Z"}),
        )
        .await;
    assert_eq!(said.error(), (403, "ScopeMissing"));
    // A new group's spaces: members and admins read meta, and members
    // read and write members; neither space is in the group's index.
    assert_eq!(
        published(&h, &did, &meta).unwrap()["policy"],
        json!({"$type": POLICY, "publicRead": true, "grants": [
            {"role": "member", "read": true},
            {"role": "admin", "read": true},
        ]})
    );
    assert_eq!(
        published(&h, &did, &members).unwrap()["policy"],
        json!({"$type": POLICY, "grants": [{"role": "member", "read": true, "write": true}]})
    );
    assert!(entry(&h, &did, &meta).is_none());
    assert!(entry(&h, &did, &members).is_none());
}

#[tokio::test]
async fn the_meta_and_members_spaces_are_never_indexed_or_hidden() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let (meta, members) = (meta_space(&did), members_space(&did));
    // `unlisted` in their policies is taken, and means nothing.
    for (space, grants) in [
        (
            &meta,
            json!([{"role": "member", "read": true}, {"role": "admin", "read": true}]),
        ),
        (
            &members,
            json!([{"role": "member", "read": true, "write": true}]),
        ),
    ] {
        let asked = json!({"$type": POLICY, "unlisted": true, "grants": grants});
        let body = json!({"space": space, "policy": asked});
        let said = method(&h, &alice, &did, space, "updateSpace", body).await;
        assert_eq!(said.status, 200, "{}", said.body);
        assert_eq!(entry(&h, &did, space), None);
    }
    // getGroup still lists both, first, to a member who reads them.
    let shown = view(&h, &did, Some(&alice)).await;
    let uris: Vec<_> = shown["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["uri"].clone())
        .collect();
    assert_eq!(uris[..2], [json!(meta), json!(members)]);
}

#[tokio::test]
async fn a_change_grants_no_more_than_its_maker_holds() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    // Hosts change any space, and give `member` alone; Bob is one.
    let host = json!({"$type": format!("{NS}.role"), "displayName": "Host", "permissions": ["space:*?manage=update"], "canAssign": ["member"]});
    let role = Write::Create {
        collection: format!("{NS}.role"),
        rkey: "host".into(),
        value: host,
    };
    let members = members_space(&did);
    h.host.space_write(&did, &members, &[role]).await.unwrap();
    set_roles(&h, &did, &bob, &["member", "host"]).await;
    // Members read the rides, admins write there; nobody reads the vault
    // but admins.
    let rides = calendar(&did, "rides");
    let both = policy(json!([
        {"role": "member", "read": true},
        {"role": "admin", "read": true, "write": true},
    ]));
    let body = json!({"spaceType": CALENDAR, "skey": "rides", "policy": both});
    let said = method(&h, &alice, &did, &rides, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let vault = calendar(&did, "vault");
    let admins = policy(json!([{"role": "admin", "read": true}]));
    let body = json!({"spaceType": CALENDAR, "skey": "vault", "policy": admins});
    let said = method(&h, &alice, &did, &vault, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);

    let change = |grants: Json| json!({"space": rides, "policy": policy(grants)});
    let admin = json!({"role": "admin", "read": true, "write": true});
    for (space, body) in [
        // A role he may not assign.
        (
            &rides,
            change(json!([{"role": "member", "read": true}, {"role": "admin", "read": true}])),
        ),
        // Writing, which he does not do there.
        (
            &rides,
            change(json!([{"role": "member", "read": true, "write": true}, admin])),
        ),
        // A scope his roles do not hold.
        (
            &rides,
            change(
                json!([{"role": "member", "read": true, "permissions": [format!("space:{CALENDAR}?collection=org.atgrp.event")]}, admin]),
            ),
        ),
        // Reading, where he does not read.
        (
            &vault,
            json!({"space": vault, "policy": policy(json!([{"role": "member", "read": true}, {"role": "admin", "read": true}]))}),
        ),
        // Anyone reading, where he does not read.
        (
            &vault,
            json!({"space": vault, "policy": {"$type": POLICY, "publicRead": true, "grants": [{"role": "admin", "read": true}]}}),
        ),
    ] {
        let said = method(&h, &bob, &did, space, "updateSpace", body.clone()).await;
        assert_eq!(said.error(), (403, "NotAuthorized"), "{body}");
    }
    // A scope he holds, for a role he gives, he may give.
    let body = change(json!([
        {"role": "member", "read": true, "permissions": [format!("space:{CALENDAR}?manage=update")]},
        admin,
    ]));
    let said = method(&h, &bob, &did, &rides, "updateSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Alice gives every role, `*`: hosts too; and, acting as the group in
    // any space, any scope a grant may hold there.
    let body = change(json!([
        {"role": "host", "read": true, "write": true},
        {"role": "member", "read": true, "permissions": [
            format!("space:{CALENDAR}?collection=org.atgrp.event"),
            format!("space:{CALENDAR}?action=read_self"),
            "space:*?manage=delete",
        ]},
        admin,
    ]));
    let said = method(&h, &alice, &did, &rides, "updateSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);

    // Every member holds `member`, which reads and writes the members
    // space, and who reads it is fixed: no change takes either from it, or
    // lets anyone read it, not even Alice's.
    let everyone = json!({"role": "member", "read": true, "write": true});
    for asked in [
        policy(json!([{"role": "member", "read": true}])),
        policy(json!([{"role": "member", "write": true}])),
        policy(json!([{"role": "admin", "read": true, "write": true}])),
        json!({"$type": POLICY, "publicRead": true, "grants": [everyone]}),
    ] {
        let body = json!({"space": members, "policy": asked});
        let said = method(&h, &alice, &did, &members, "updateSpace", body).await;
        assert_eq!(said.error(), (400, "MembersSpaceFixed"), "{asked}");
    }
    let kept = policy(
        json!([{"role": "member", "read": true, "write": true, "permissions": [
            format!("space:{NS}.members?collection={NS}.confirmation"),
        ]}]),
    );
    let body = json!({"space": members, "policy": kept});
    let said = method(&h, &alice, &did, &members, "updateSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
}

#[tokio::test]
async fn an_admin_ends_a_space() {
    // On http, so that it calls syncers' endpoints here.
    let h = start(&[("PUBLIC_URL", "http://host.test")], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let rides = calendar(&did, "rides");
    let readers = policy(json!([{"role": "member", "read": true}]));
    let body = json!({"spaceType": CALENDAR, "skey": "rides", "policy": readers});
    let said = method(&h, &alice, &did, &rides, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let picture = h.host.upload_blob(&did, b"map", "image/png").unwrap();
    h.host
        .space_write(
            &did,
            &rides,
            &[Write::Create {
                collection: "org.atgrp.event".into(),
                rkey: "first".into(),
                value: json!({"$type": "org.atgrp.event", "name": "First ride", "map": picture}),
            }],
        )
        .await
        .unwrap();
    let (url, syncer) = endpoint().await;
    let to = Registration {
        endpoint: url,
        token: "tok".into(),
    };
    h.host
        .store
        .register(&rides, &to, now() as i64 + 60)
        .unwrap();

    let gone = json!({ "space": rides });
    let said = method(&h, &bob, &did, &rides, "deleteSpace", gone.clone()).await;
    assert_eq!(said.error(), (403, "Forbidden"));
    let said = method(
        &h,
        &alice,
        &did,
        &meta_space(&did),
        "deleteSpace",
        gone.clone(),
    )
    .await;
    assert_eq!(said.error(), (403, "ScopeMissing"));
    let said = method(&h, &alice, &did, &rides, "deleteSpace", gone.clone()).await;
    assert_eq!((said.status, said.body), (200, json!({})));
    assert_eq!(entry(&h, &did, &rides), None);
    assert!(h.host.store.space(&rides).unwrap().is_none());
    assert!(h.host.store.space_head(&did, &rides).unwrap().is_none());
    let cid = atproto::Cid::parse(picture["ref"]["$link"].as_str().unwrap()).unwrap();
    assert_eq!(h.host.store.blob(&did, &cid).unwrap(), None);
    // Its syncers are told, and it says it was deleted.
    let told = syncer
        .told_of("com.atproto.space.notifySpaceUpdate#spaceDeleted", 1)
        .await;
    assert_eq!(
        told,
        [
            json!({"space": rides, "token": "tok", "update": {"$type": "com.atproto.space.notifySpaceUpdate#spaceDeleted"}})
        ]
    );
    assert!(h.host.store.space_deleted(&rides).unwrap());
    assert_eq!(h.host.store.registration(&rides, &to).unwrap(), None);
    let said = get_space(&h, &alice, &did, &rides).await;
    assert_eq!(said.error(), (400, "SpaceNotFound"));

    // The meta and members spaces stay as long as the group.
    for space in [meta_space(&did), members_space(&did)] {
        let body = json!({ "space": space });
        let said = method(&h, &alice, &did, &space, "deleteSpace", body).await;
        assert_eq!(said.error(), (403, "NotAuthorized"));
    }
    // One left out of the index ends as well, by the grant in its policy
    // to delete it; and one whose ender lost that grant is not ended.
    let hidden = calendar(&did, "hidden");
    let deleters = json!({"$type": POLICY, "unlisted": true, "grants": [{"role": "member", "read": true, "permissions": [format!("space:{CALENDAR}?manage=delete")]}]});
    let body = json!({"spaceType": CALENDAR, "skey": "hidden", "policy": deleters});
    let said = method(&h, &alice, &did, &hidden, "createSpace", body).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let app = App::new("console");
    let token = app.token_for(&h, &bob, &did, &hidden).await;
    set_roles(&h, &did, &bob, &[]).await;
    let body = json!({ "space": hidden });
    let said = app
        .call(&h, "com.atproto.space.deleteSpace", &token, Some(&body))
        .await;
    assert_eq!(said.error(), (403, "NotAuthorized"));
    set_roles(&h, &did, &bob, &["member"]).await;
    let said = app
        .call(&h, "com.atproto.space.deleteSpace", &token, Some(&body))
        .await;
    assert_eq!((said.status, said.body), (200, json!({})));
    assert!(h.host.store.space(&hidden).unwrap().is_none());
    let said = app
        .call(&h, "com.atproto.space.deleteSpace", &token, Some(&body))
        .await;
    assert_eq!(said.error(), (400, "SpaceNotFound"));
}
