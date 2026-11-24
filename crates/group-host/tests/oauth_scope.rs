//! What a token signing in as a group may do: the scope asked for,
//! narrowed to the approving member's roles, and made concrete.

mod common;

use atproto::space::SpaceRef;
use common::groups::*;
use common::oauth::App;
use common::spaces::*;
use common::{Running, start};
use group_host::group::{members_space, meta_space};
use group_host::oauth_scope::{SpaceScope, concrete};
use group_host::rbac::{Grant, Policy};
use group_host::repo::Write;
use serde_json::json;

const CALENDAR: &str = "com.example.calendar";
const EVENT: &str = "com.example.event";

/// Those of `scopes` for the calendar.
fn calendar(scopes: Vec<String>) -> Vec<String> {
    scopes
        .into_iter()
        .filter(|s| s.contains(CALENDAR))
        .collect()
}

fn roles(r: &[&str]) -> Vec<String> {
    r.iter().map(|r| r.to_string()).collect()
}

fn asked(scopes: &[&str]) -> Vec<String> {
    roles(scopes)
}

/// Makes `did` the role `grant` is of, with that grant.
async fn define(h: &Running, did: &str, grant: Grant) {
    let value = json!({"$type": format!("{NS}.role"), "displayName": grant.role, "scopes": grant.scopes, "assignable": grant.assignable});
    let write = Write::Create {
        collection: format!("{NS}.role"),
        rkey: grant.role,
        value,
    };
    h.host
        .space_write(did, &members_space(did), &[write])
        .await
        .unwrap();
}

/// A group whose `poster` role may write posts in its repo, and a
/// calendar members read, where members add events as the group and
/// admins write anything.
async fn group(h: &Running) -> (String, String, Authority) {
    let [alice, ..] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let policy = json!({"$type": "fyi.opensocial.policy", "grants": [
        {"role": "member", "read": true, "scopes": [format!("space:{CALENDAR}?collection={EVENT}&action=create")]},
        {"role": "admin", "read": true, "scopes": [format!("space:{CALENDAR}?collection=*")]},
    ]});
    let made = h
        .host
        .create_space(&did, CALENDAR, "rides", &policy)
        .unwrap();
    define(
        h,
        &did,
        Grant::with("poster", &[], &[], &["app.bsky.feed.post"]),
    )
    .await;
    (did, made, alice)
}

#[tokio::test]
async fn keeps_for_each_role_what_its_grants_hold() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    let all = asked(&[
        "atproto",
        "include:fyi.opensocial.authManageGroup",
        "include:com.example.other",
        "repo:app.bsky.feed.post?action=create",
        "repo:app.bsky.feed.like",
        "rpc:app.bsky.feed.getTimeline?aud=*",
        "rpc:fyi.opensocial.listModSubjects?aud=*",
        "rpc:*?aud=*",
        "blob:*/*",
        "transition:generic",
    ]);
    let admin = h.host.narrow(&did, &roles(&["admin"]), &all).unwrap();
    assert_eq!(
        admin,
        asked(&[
            "atproto",
            "include:fyi.opensocial.authManageGroup",
            "repo:app.bsky.feed.post?action=create",
            "repo:app.bsky.feed.like",
            "rpc:app.bsky.feed.getTimeline?aud=*",
            "blob:*/*",
        ])
    );
    let poster = h.host.narrow(&did, &roles(&["poster"]), &all).unwrap();
    assert_eq!(
        poster,
        asked(&[
            "atproto",
            "repo:app.bsky.feed.post?action=create",
            "blob:*/*"
        ])
    );
    // A plain member writes nothing, so has no use for blobs.
    let member = h.host.narrow(&did, &roles(&["member"]), &all).unwrap();
    assert_eq!(member, asked(&["atproto"]));
}

#[tokio::test]
async fn reads_every_form_a_scope_may_take() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    // Collections named by query, files by type, and values escaped, as
    // an app's URLSearchParams writes them.
    let encoded = format!("space:{CALENDAR}?authority={}", did.replace(':', "%3A"));
    let all = asked(&[
        "atproto",
        "repo?collection=app.bsky.feed.post&collection=app.bsky.feed.like",
        "repo?collection=app.bsky.feed.post",
        "blob?accept=image/*",
        &encoded,
    ]);
    let admin = h.host.narrow(&did, &roles(&["admin"]), &all).unwrap();
    assert_eq!(admin, all);
    // Kept only when every collection it names is one the roles write.
    let poster = h.host.narrow(&did, &roles(&["poster"]), &all).unwrap();
    assert_eq!(
        poster,
        asked(&[
            "atproto",
            "repo?collection=app.bsky.feed.post",
            "blob?accept=image/*"
        ])
    );
}

#[tokio::test]
async fn keeps_a_space_scope_a_spaces_policy_grants() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    let member = |scopes: &[&str]| {
        h.host
            .narrow(&did, &roles(&["member"]), &asked(scopes))
            .unwrap()
    };
    for (scope, kept) in [
        (
            format!("space:{CALENDAR}?collection={EVENT}&action=create"),
            true,
        ),
        (
            format!("space:{CALENDAR}?authority=self&skey=rides&collection={EVENT}&action=create"),
            true,
        ),
        (
            format!("space:{CALENDAR}?authority={did}&collection={EVENT}&action=create"),
            true,
        ),
        (
            format!("space:{CALENDAR}?collection={EVENT}&action=delete"),
            false,
        ),
        (format!("space:{CALENDAR}?collection={EVENT}"), false),
        (
            format!("space:{CALENDAR}?collection=com.example.rsvp&action=create"),
            false,
        ),
        (format!("space:{CALENDAR}?action=read"), true),
        (
            format!("space:{CALENDAR}?action=read&action=read_self"),
            true,
        ),
        (format!("space:{CALENDAR}?skey=other&action=read"), false),
        (
            format!("space:{CALENDAR}?authority=did:plc:other&action=read"),
            false,
        ),
        ("space:com.example.forum?action=read".to_string(), false),
    ] {
        let narrowed = member(&["atproto", &scope, "blob:*/*"]);
        let expected = if !kept {
            vec!["atproto".to_string()]
        } else if scope.contains("action=read") && !scope.contains("create") {
            vec!["atproto".to_string(), scope.clone()]
        } else {
            vec!["atproto".to_string(), scope.clone(), "blob:*/*".into()]
        };
        assert_eq!(narrowed, expected, "{scope}");
    }
    // Every collection, and every write, for a role granted `*`.
    let admin = h
        .host
        .narrow(
            &did,
            &roles(&["admin"]),
            &asked(&[&format!("space:{CALENDAR}")]),
        )
        .unwrap();
    assert_eq!(admin, [format!("space:{CALENDAR}")]);
}

#[tokio::test]
async fn makes_a_scope_for_every_type_one_for_each_space_the_roles_read() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    // The group's own spaces are among them, read by members.
    let all = h
        .host
        .narrow(&did, &roles(&["member"]), &asked(&["space:*?action=read"]))
        .unwrap();
    assert_eq!(
        all,
        [
            format!("space:{CALENDAR}?authority={did}&skey=rides&action=read"),
            format!("space:fyi.opensocial.members?authority={did}&skey=self&action=read"),
            format!("space:fyi.opensocial.meta?authority={did}&skey=self&action=read"),
        ]
    );
    let narrow = |r: &[&str], scope: &str| {
        calendar(h.host.narrow(&did, &roles(r), &asked(&[scope])).unwrap())
    };
    let at = format!("space:{CALENDAR}?authority={did}&skey=rides");
    assert_eq!(
        narrow(
            &["member"],
            &format!("space:*?collection={EVENT}&action=create")
        ),
        [format!("{at}&collection={EVENT}&action=read&action=create")]
    );
    // Asked for more than the role writes: only what it writes.
    assert_eq!(
        narrow(&["member"], "space:*"),
        [format!("{at}&collection={EVENT}&action=read&action=create")]
    );
    assert_eq!(
        narrow(&["member"], "space:*?action=read"),
        [format!("{at}&action=read")]
    );
    // Asked for a write the role has not, it gets nothing there.
    assert_eq!(
        narrow(
            &["member"],
            &format!("space:*?collection={EVENT}&action=delete")
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        narrow(&["member"], "space:*?collection=com.example.rsvp"),
        Vec::<String>::new()
    );
    assert_eq!(
        narrow(&["member"], "space:*?skey=other&action=read"),
        Vec::<String>::new()
    );
    // A grant of every collection is as wide as what was asked, and of an
    // app's records only in its own kind of space.
    assert_eq!(
        narrow(&["admin", "member"], "space:*"),
        [
            format!("{at}&collection=*&action=read&action=create&action=update&action=delete"),
            format!("{at}&collection={EVENT}&action=read&action=create"),
        ]
    );
    assert_eq!(
        narrow(
            &["admin", "member"],
            &format!("space:*?collection={EVENT}&collection=dev.other.thing&action=update")
        ),
        [format!("{at}&collection={EVENT}&action=read&action=update")]
    );
    // Nothing where the roles read nothing.
    assert_eq!(
        narrow(&["poster"], "space:*?action=read"),
        Vec::<String>::new()
    );
    assert_eq!(
        narrow(&["admin"], "space:*?authority=did:plc:other"),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn keeps_a_manage_scope_for_roles_that_may_do_it_to_spaces() {
    let h = start(&[], false).await;
    let (did, uri, _) = group(&h).await;
    let narrow = |r: &[&str], scope: &str| {
        h.host
            .narrow(&did, &roles(r), &asked(&[scope]))
            .unwrap()
            .len()
            == 1
    };
    // Admins make, change and end any space; members none.
    for scope in [
        "space:*?manage=create".to_string(),
        format!("space:{CALENDAR}?manage=update&manage=delete"),
    ] {
        assert!(narrow(&["admin"], &scope), "{scope}");
        assert!(!narrow(&["member"], &scope), "{scope}");
    }
    // A space's policy lets members change it, and so spaces of its
    // type, that one by its key; none other, and never ending it.
    let policy = json!({"$type": "fyi.opensocial.policy", "grants": [
        {"role": "member", "read": true, "scopes": [format!("space:{CALENDAR}?manage=update")]},
    ]});
    h.host.store.set_policy(&uri, &policy).unwrap();
    for (scope, kept) in [
        (format!("space:{CALENDAR}?manage=update"), true),
        (format!("space:{CALENDAR}?skey=rides&manage=update"), true),
        (format!("space:{CALENDAR}?skey=laps&manage=update"), false),
        ("space:com.example.forum?manage=update".to_string(), false),
        (format!("space:{CALENDAR}?manage=delete"), false),
        (format!("space:{CALENDAR}?manage=create"), false),
    ] {
        assert_eq!(narrow(&["member"], &scope), kept, "{scope}");
    }
}

#[tokio::test]
async fn gives_nothing_to_write_for_a_role_without_its_record() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    let gone = Write::Delete {
        collection: format!("{NS}.role"),
        rkey: "admin".into(),
    };
    h.host
        .space_write(&did, &members_space(&did), &[gone])
        .await
        .unwrap();
    let narrowed = h
        .host
        .narrow(
            &did,
            &roles(&["admin"]),
            &asked(&["atproto", "include:fyi.opensocial.authModerate", "repo:x"]),
        )
        .unwrap();
    assert_eq!(narrowed, asked(&["atproto"]));
}

#[test]
fn makes_a_grant_concrete() {
    let did = "did:plc:riders";
    let scope = concrete(
        &asked(&[
            "atproto",
            "include:fyi.opensocial.authManageProfile",
            "space:com.example.calendar?authority=self&skey=rides",
            "space:com.example.calendar",
            "space:com.example.calendar?action=read",
            "atproto",
        ]),
        did,
    );
    let scopes: Vec<&str> = scope.split(' ').collect();
    assert_eq!(
        scopes,
        [
            "atproto",
            "space:fyi.opensocial.meta?authority=did:plc:riders&skey=self&collection=fyi.opensocial.profile&collection=fyi.opensocial.rule&action=read&action=create&action=update&action=delete",
            "space:com.example.calendar?authority=did:plc:riders&skey=rides&collection=*",
            "space:com.example.calendar?collection=*",
            "space:com.example.calendar?action=read",
        ]
    );
}

/// Sets `did`'s meta space's policy: as a new group's, and `role` writing
/// its profile and rules as the group.
fn profile_writer(h: &Running, did: &str, role: &str) {
    let write = format!("space:{NS}.meta?collection={NS}.profile&collection={NS}.rule");
    let policy = json!({"$type": "fyi.opensocial.policy", "grants": [
        {"role": "member", "read": true},
        {"role": "admin", "read": true, "scopes": [write]},
        {"role": role, "read": true, "scopes": [write]},
    ]});
    h.host.store.set_policy(&meta_space(did), &policy).unwrap();
}

#[tokio::test]
async fn keeps_of_a_permission_set_what_the_roles_grant() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    let moderate = format!("include:{NS}.authModerate");
    define(
        &h,
        &did,
        Grant {
            role: "moderator".into(),
            scopes: vec![moderate],
            assignable: vec![],
        },
    )
    .await;
    for (role, actions) in [
        ("doorkeeper", &["admit", "eject"][..]),
        ("steward", &["space.configure"][..]),
        ("editor", &[][..]),
    ] {
        define(&h, &did, Grant::with(role, actions, &[], &[])).await;
    }
    profile_writer(&h, &did, "editor");
    let narrow =
        |r: &[&str], scopes: &[&str]| h.host.narrow(&did, &roles(r), &asked(scopes)).unwrap();
    let group = "include:fyi.opensocial.authManageGroup";
    let moderation = "include:fyi.opensocial.authModerate";
    let profile = "include:fyi.opensocial.authManageProfile";
    // Whole, for roles that grant all of it.
    for set in [group, moderation, profile] {
        assert_eq!(narrow(&["admin"], &[set]), [set], "{set}");
    }
    // A moderator asked to run the group gets moderation: each tier
    // holds the one below it.
    assert_eq!(narrow(&["moderator"], &[group]), [moderation]);
    assert_eq!(narrow(&["moderator"], &[moderation]), [moderation]);
    assert_eq!(narrow(&["moderator"], &[profile]), Vec::<String>::new());
    // A role that writes the profile and rules by the meta space's
    // policy gets the profile, from any tier.
    for set in [group, profile] {
        assert_eq!(narrow(&["editor"], &[set]), [profile], "{set}");
    }
    assert_eq!(narrow(&["editor"], &[moderation]), Vec::<String>::new());
    // Less than any tier: what of it the roles grant, as scopes.
    let all = "action=read&action=create&action=update&action=delete";
    assert_eq!(
        narrow(&["doorkeeper"], &[moderation, "blob:*/*"]),
        [
            format!(
                "space:{NS}.members?authority=self&skey=self&collection={NS}.membership&action=read&action=create&action=delete"
            ),
            format!("rpc:{NS}.listJoinRequests?aud=*"),
            format!("rpc:{NS}.denyJoinRequest?aud=*"),
            "blob:*/*".into(),
        ]
    );
    assert_eq!(
        narrow(&["steward"], &[group]),
        [
            format!("space:{NS}.meta?authority=self&skey=self&action=read&manage=update"),
            format!("space:{NS}.members?authority=self&skey=self&action=read&manage=update"),
        ]
    );
    assert_eq!(
        narrow(&["editor", "doorkeeper"], &[group]),
        [
            format!(
                "space:{NS}.meta?authority=self&skey=self&collection={NS}.profile&collection={NS}.rule&{all}"
            ),
            format!(
                "space:{NS}.members?authority=self&skey=self&collection={NS}.membership&action=read&action=create&action=delete"
            ),
            format!("rpc:{NS}.listJoinRequests?aud=*"),
            format!("rpc:{NS}.denyJoinRequest?aud=*"),
        ]
    );
    assert_eq!(
        narrow(&["member"], &[group, "blob:*/*"]),
        Vec::<String>::new()
    );
    // A person's own set is never kept for a group's session, nor what is
    // no set of ours.
    for role in ["admin", "moderator", "member"] {
        let others = [
            "include:fyi.opensocial.authParticipate",
            "include:fyi.opensocial.profile",
        ];
        assert_eq!(narrow(&[role], &others), Vec::<String>::new(), "{role}");
    }
}

#[tokio::test]
async fn a_profile_writer_keeps_the_profile_with_what_it_approved() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    define(&h, &did, Grant::with("editor", &[], &[], &[])).await;
    profile_writer(&h, &did, "editor");
    let [_, bob, _] = people(&h).await;
    let membership = json!({"$type": format!("{NS}.membership"), "roles": ["member", "editor"], "createdAt": "2026-11-03T00:00:00Z"});
    let write = Write::Create {
        collection: format!("{NS}.membership"),
        rkey: bob.did.clone(),
        value: membership,
    };
    h.host
        .space_write(&did, &members_space(&did), &[write])
        .await
        .unwrap();
    let app = App::loopback("atproto include:fyi.opensocial.authManageGroup");
    let tokens = app.sign_in(&h, &bob, &did).await;
    assert!(
        tokens["scope"]
            .as_str()
            .unwrap()
            .contains(&format!("collection={NS}.profile")),
        "{tokens}"
    );
    let token = tokens["access_token"].as_str().unwrap();
    let meta = meta_space(&did);
    let profile = json!({"$type": format!("{NS}.profile"), "displayName": "Bob's riders", "joinPolicy": "open"});
    let said = app
        .put(&h, token, &meta, &format!("{NS}.profile"), "self", profile)
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // Its labeler is the group's administration's, not its profile's.
    let labeler = h
        .host
        .group_record(&did, &meta, &format!("{NS}.labeler"), "self")
        .unwrap()
        .unwrap();
    let said = app
        .put(&h, token, &meta, &format!("{NS}.labeler"), "self", labeler)
        .await;
    assert_eq!(said.status, 403, "{}", said.body);
    // Alice, an admin, keeps all of it.
    let alices = App::loopback("atproto include:fyi.opensocial.authManageGroup");
    let tokens = alices.sign_in(&h, &alice, &did).await;
    let scope = tokens["scope"].as_str().unwrap();
    assert!(scope.contains(&format!("collection={NS}.role")), "{scope}");
}

#[tokio::test]
async fn keeps_an_inbox_scope_for_roles_that_may_invite() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    define(&h, &did, Grant::with("host", &["invite"], &[], &[])).await;
    let any = format!("space:{NS}.invites?collection={NS}.invite&action=create");
    let bobs = format!("space:{NS}.invites?authority=did:plc:bob&collection={NS}.invite");
    let other = format!("space:{NS}.invites?collection={NS}.rule");
    let wanted = [any.as_str(), bobs.as_str(), other.as_str(), "blob:image/*"];
    let narrow = |r: &[&str]| {
        let wanted = asked(&wanted);
        h.host.narrow(&did, &roles(r), &wanted).unwrap()
    };
    // Invites, in anyone's inbox or one person's, and nothing else there.
    assert_eq!(
        narrow(&["host"]),
        asked(&[any.as_str(), bobs.as_str(), "blob:image/*"])
    );
    assert_eq!(narrow(&["member"]), Vec::<String>::new());
}

#[test]
fn makes_a_permission_set_concrete() {
    let did = "did:plc:riders";
    let scope = concrete(&asked(&["include:fyi.opensocial.authManageGroup"]), did);
    let scopes: Vec<&str> = scope.split(' ').collect();
    let all = "action=read&action=create&action=update&action=delete";
    let at = format!("authority={did}&skey=self");
    // Writing records and changing the space are scopes of their own.
    assert_eq!(
        scopes,
        [
            format!(
                "space:{NS}.meta?{at}&collection={NS}.profile&collection={NS}.rule&collection={NS}.labeler&collection=com.atproto.label.record&{all}"
            ),
            format!("space:{NS}.meta?{at}&action=read&manage=update"),
            format!(
                "space:{NS}.members?{at}&collection={NS}.role&collection={NS}.membership&collection=com.atproto.label.record&{all}"
            ),
            format!("space:{NS}.members?{at}&action=read&manage=update"),
            format!(
                "space:{NS}.invites?authority=*&skey=self&collection={NS}.invite&action=read_self&action=create&action=update&action=delete"
            ),
            format!("rpc:{NS}.listJoinRequests?aud=*"),
            format!("rpc:{NS}.denyJoinRequest?aud=*"),
            format!("rpc:{NS}.listInvites?aud=*"),
            format!("rpc:{NS}.listModSubjects?aud=*"),
            format!("rpc:{NS}.getModSubjectHistory?aud=*"),
            format!("rpc:{NS}.getModSubjectBlob?aud=*"),
            format!("rpc:{NS}.resolveModSubject?aud=*"),
        ]
    );
}

#[test]
fn reads_a_space_scope() {
    let s = SpaceScope::parse("space:com.example.calendar?skey=rides&collection=a&collection=b&action=create&manage=create").unwrap();
    assert_eq!(s.kind, "com.example.calendar");
    assert_eq!((s.authority.as_str(), s.skey.as_str()), ("*", "rides"));
    assert_eq!(s.collections, ["a", "b"]);
    assert_eq!(s.writes, ["create"]);
    assert!(!s.read_only);
    let s = SpaceScope::parse("space:*").unwrap();
    assert_eq!(s.writes, ["create", "update", "delete"]);
    assert!(!s.read_only);
    assert_eq!(SpaceScope::parse("repo:x"), None);
}

#[test]
fn unions_a_roles_writes() {
    let space = SpaceRef::parse("at://did:plc:g/space/com.example.calendar/rides").unwrap();
    let at = "space:com.example.calendar";
    let policy = json!({"grants": [
        {"role": "a", "scopes": [format!("{at}?collection=x&action=create"), format!("{at}?collection=y")]},
        {"role": "b", "scopes": [format!("{at}?collection=x&action=delete&action=other")]},
        {"role": "c", "scopes": [format!("{at}?collection=z")]},
    ]});
    let writes = Policy::of(&policy, &space).writes(&roles(&["a", "b"]));
    let x: Vec<&str> = writes["x"].iter().map(String::as_str).collect();
    assert_eq!(x, ["create", "delete"]);
    assert_eq!(writes["y"].len(), 3);
    assert!(!writes.contains_key("z"));
    assert!(
        Policy::of(&json!({}), &space)
            .writes(&roles(&["a"]))
            .is_empty()
    );
}
