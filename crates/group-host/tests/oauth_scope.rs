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
    let value = json!({"$type": format!("{NS}.role"), "displayName": grant.role, "permissions": grant.permissions, "canAssign": grant.can_assign});
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
    let policy = json!({"$type": "org.atgrp.policy", "grants": [
        {"role": "member", "read": true, "permissions": [format!("space:{CALENDAR}?collection={EVENT}&action=create")]},
        {"role": "admin", "read": true, "permissions": [format!("space:{CALENDAR}?collection=*")]},
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
        "include:org.atgrp.authManageAdmin",
        "include:com.example.other",
        "repo:app.bsky.feed.post?action=create",
        "repo:app.bsky.feed.like",
        "rpc:app.bsky.feed.getTimeline?aud=*",
        "rpc:org.atgrp.mod.querySubjects?aud=*",
        "rpc:*?aud=*",
        "blob:*/*",
        "transition:generic",
    ]);
    let admin = h
        .host
        .narrow(&did, &roles(&["admin"]), &all, &Default::default())
        .unwrap();
    assert_eq!(
        admin,
        asked(&[
            "atproto",
            "include:org.atgrp.authManageAdmin",
            "repo:app.bsky.feed.post?action=create",
            "repo:app.bsky.feed.like",
            "rpc:app.bsky.feed.getTimeline?aud=*",
            // One of the host's own, as an admin holds mod.read.
            "rpc:org.atgrp.mod.querySubjects?aud=*",
            "blob:*/*",
            // As an app password's maker, who holds repo:*.
            "transition:generic",
        ])
    );
    let poster = h
        .host
        .narrow(&did, &roles(&["poster"]), &all, &Default::default())
        .unwrap();
    assert_eq!(
        poster,
        asked(&[
            "atproto",
            "repo:app.bsky.feed.post?action=create",
            "blob:*/*"
        ])
    );
    // A plain member writes nothing, so has no use for blobs.
    let member = h
        .host
        .narrow(&did, &roles(&["member"]), &all, &Default::default())
        .unwrap();
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
    let admin = h
        .host
        .narrow(&did, &roles(&["admin"]), &all, &Default::default())
        .unwrap();
    assert_eq!(admin, all);
    // Kept only when every collection it names is one the roles write.
    let poster = h
        .host
        .narrow(&did, &roles(&["poster"]), &all, &Default::default())
        .unwrap();
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
            .narrow(
                &did,
                &roles(&["member"]),
                &asked(scopes),
                &Default::default(),
            )
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
            &Default::default(),
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
        .narrow(
            &did,
            &roles(&["member"]),
            &asked(&["space:*?action=read"]),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(
        all,
        [
            format!("space:{CALENDAR}?authority={did}&skey=rides&action=read"),
            format!("space:org.atgrp.members?authority={did}&skey=self&action=read"),
            format!("space:org.atgrp.meta?authority={did}&skey=self&action=read"),
        ]
    );
    let narrow = |r: &[&str], scope: &str| {
        calendar(
            h.host
                .narrow(&did, &roles(r), &asked(&[scope]), &Default::default())
                .unwrap(),
        )
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
    // Asked for one of them as well, it is granted once.
    let read = format!("{at}&action=read");
    let twice = h
        .host
        .narrow(
            &did,
            &roles(&["member"]),
            &asked(&[&read, "space:*?action=read"]),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(calendar(twice), [read]);
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
            .narrow(&did, &roles(r), &asked(&[scope]), &Default::default())
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
    let policy = json!({"$type": "org.atgrp.policy", "grants": [
        {"role": "member", "read": true, "permissions": [format!("space:{CALENDAR}?manage=update")]},
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
            &asked(&["atproto", "include:org.atgrp.authModerate", "repo:x"]),
            &Default::default(),
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
            "include:org.atgrp.authManageBasic",
            "space:com.example.calendar?authority=self&skey=rides",
            "space:com.example.calendar",
            "space:com.example.calendar?action=read",
            "atproto",
        ]),
        did,
        &Default::default(),
    );
    let scopes: Vec<&str> = scope.split(' ').collect();
    assert_eq!(
        scopes,
        [
            "atproto",
            "space:org.atgrp.meta?authority=did:plc:riders&skey=self&collection=org.atgrp.about&action=read&action=create&action=update&action=delete",
            "space:org.atgrp.members?authority=did:plc:riders&skey=self&collection=org.atgrp.rule&action=read&action=create&action=update&action=delete",
            "space:com.example.calendar?authority=did:plc:riders&skey=rides&collection=*",
            "space:com.example.calendar?collection=*",
            "space:com.example.calendar?action=read",
        ]
    );
}

#[tokio::test]
async fn keeps_of_a_permission_set_what_the_roles_grant() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    let set = |name: &str| format!("include:{NS}.{name}");
    for (role, permissions) in [
        ("moderator", set("authModerate")),
        ("editor", set("authManageBasic")),
    ] {
        let grant = Grant {
            role: role.into(),
            permissions: vec![permissions],
            can_assign: vec![],
        };
        define(&h, &did, grant).await;
    }
    for (role, actions) in [
        ("doorkeeper", &["admit", "eject"][..]),
        ("steward", &["space.configure"][..]),
    ] {
        define(&h, &did, Grant::with(role, actions, &[], &[])).await;
    }
    let narrow = |r: &[&str], scopes: &[&str]| {
        h.host
            .narrow(&did, &roles(r), &asked(scopes), &Default::default())
            .unwrap()
    };
    let admin = "include:org.atgrp.authManageAdmin";
    let members = "include:org.atgrp.authManageMembers";
    let moderation = "include:org.atgrp.authModerate";
    let basic = "include:org.atgrp.authManageBasic";
    // Whole, for roles that grant all of it.
    for set in [admin, members, moderation, basic] {
        assert_eq!(narrow(&["admin"], &[set]), [set], "{set}");
    }
    // A moderator asked to run the group gets moderation, which the admin
    // set holds; of running its members, putting them out.
    assert_eq!(narrow(&["moderator"], &[admin]), [moderation]);
    assert_eq!(narrow(&["moderator"], &[moderation]), [moderation]);
    assert_eq!(narrow(&["moderator"], &[basic]), Vec::<String>::new());
    assert_eq!(
        narrow(&["moderator"], &[members]),
        [format!(
            "space:{NS}.members?authority=self&skey=self&collection={NS}.membership&action=read&action=delete"
        )]
    );
    // A role holding the basics, the about record and rules, gets them
    // from the admin set too.
    for set in [admin, basic] {
        assert_eq!(narrow(&["editor"], &[set]), [basic], "{set}");
    }
    assert_eq!(narrow(&["editor"], &[moderation]), Vec::<String>::new());
    // Less than any set: what of it the roles grant, as scopes.
    let all = "action=read&action=create&action=update&action=delete";
    assert_eq!(
        narrow(&["doorkeeper"], &[members, "blob:*/*"]),
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
        narrow(&["steward"], &[admin]),
        [
            format!("space:{NS}.meta?authority=self&skey=self&action=read&manage=update"),
            format!("space:{NS}.members?authority=self&skey=self&action=read&manage=update"),
        ]
    );
    assert_eq!(
        narrow(&["editor", "doorkeeper"], &[admin]),
        [
            format!("space:{NS}.meta?authority=self&skey=self&collection={NS}.about&{all}"),
            format!("space:{NS}.members?authority=self&skey=self&collection={NS}.rule&{all}"),
            format!(
                "space:{NS}.members?authority=self&skey=self&collection={NS}.membership&action=read&action=create&action=delete"
            ),
            format!("rpc:{NS}.listJoinRequests?aud=*"),
            format!("rpc:{NS}.denyJoinRequest?aud=*"),
        ]
    );
    assert_eq!(
        narrow(&["member"], &[admin, "blob:*/*"]),
        Vec::<String>::new()
    );
    // A person's own set is never kept for a group's session, nor what is
    // no set of ours.
    for role in ["admin", "moderator", "member"] {
        let others = [
            "include:org.atgrp.authParticipate",
            "include:org.atgrp.about",
        ];
        assert_eq!(narrow(&[role], &others), Vec::<String>::new(), "{role}");
    }
}

#[tokio::test]
async fn keeps_labels_only_in_the_spaces_the_roles_label_in() {
    let h = start(&[], false).await;
    let (did, calendar_uri, _) = group(&h).await;
    let moderate = format!("include:{NS}.authModerate");
    let label = "com.atproto.label.record";
    let every = format!("space:*?collection={label}");
    define(
        &h,
        &did,
        Grant {
            role: "moderator".into(),
            permissions: vec![moderate.clone()],
            can_assign: vec![],
        },
    )
    .await;
    define(
        &h,
        &did,
        Grant {
            role: "labeller".into(),
            permissions: vec![moderate, every.clone()],
            can_assign: vec![],
        },
    )
    .await;
    let narrow = |r: &[&str], scopes: &[&str]| {
        h.host
            .narrow(&did, &roles(r), &asked(scopes), &Default::default())
            .unwrap()
    };
    let named = format!("space:{CALENDAR}?collection={label}");
    // The moderation set's labels are the meta and members spaces'.
    let moderator = narrow(&["member", "moderator"], &[&every]);
    for kind in ["meta", "members"] {
        let there = |s: &String| s.contains(&format!("{NS}.{kind}?")) && s.contains(label);
        assert!(moderator.iter().any(there), "{kind}: {moderator:?}");
    }
    assert_eq!(calendar(moderator), Vec::<String>::new());
    assert_eq!(narrow(&["moderator"], &[&named]), Vec::<String>::new());
    // Labels in every space reach the calendar.
    let labeller = narrow(&["member", "labeller"], &[&every]);
    assert_eq!(calendar(labeller).len(), 1);
    let kept = narrow(&["labeller"], &[&named]);
    assert_eq!(kept, std::slice::from_ref(&named));
    // As does a grant of them in its policy, there only.
    let policy = json!({"$type": "org.atgrp.policy", "grants": [
        {"role": "member", "read": true},
        {"role": "moderator", "read": true, "permissions": [every]},
    ]});
    h.host.store.set_policy(&calendar_uri, &policy).unwrap();
    let moderator = narrow(&["member", "moderator"], &[&every]);
    assert_eq!(calendar(moderator).len(), 1);
    assert_eq!(narrow(&["moderator"], &[&named]), [named]);
}

#[tokio::test]
async fn a_holder_of_the_basics_keeps_the_about_record_and_rules_with_what_it_approved() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    let basics = Grant {
        role: "editor".into(),
        permissions: vec![format!("include:{NS}.authManageBasic")],
        can_assign: vec![],
    };
    define(&h, &did, basics).await;
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
    let app = App::loopback("atproto include:org.atgrp.authManageAdmin");
    let tokens = app.sign_in(&h, &bob, &did).await;
    let scope = tokens["scope"].as_str().unwrap();
    for collection in ["about", "rule"] {
        assert!(
            scope.contains(&format!("collection={NS}.{collection}")),
            "{scope}"
        );
    }
    let token = tokens["access_token"].as_str().unwrap();
    let (meta, members) = (meta_space(&did), members_space(&did));
    let about = json!({"$type": format!("{NS}.about"), "displayName": "Bob's riders"});
    let said = app
        .put(&h, token, &meta, &format!("{NS}.about"), "self", about)
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let rule = json!({"$type": format!("{NS}.rule"), "title": "Ride safe", "text": "Helmets on."});
    let said = app
        .put(
            &h,
            token,
            &members,
            &format!("{NS}.rule"),
            "3jzfcijpj2z2a",
            rule,
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    // The config and labeler are the group's administration's, not the
    // basics'.
    let config = json!({"$type": format!("{NS}.config"), "joinPolicy": "open"});
    let said = app
        .put(&h, token, &meta, &format!("{NS}.config"), "self", config)
        .await;
    assert_eq!(said.status, 403, "{}", said.body);
    let labeler = h
        .host
        .group_record(&did, &members, &format!("{NS}.labeler"), "self")
        .unwrap()
        .unwrap();
    let said = app
        .put(
            &h,
            token,
            &members,
            &format!("{NS}.labeler"),
            "self",
            labeler,
        )
        .await;
    assert_eq!(said.status, 403, "{}", said.body);
    // Alice, an admin, keeps all of it.
    let alices = App::loopback("atproto include:org.atgrp.authManageAdmin");
    let tokens = alices.sign_in(&h, &alice, &did).await;
    let scope = tokens["scope"].as_str().unwrap();
    for collection in ["role", "config", "labeler"] {
        assert!(
            scope.contains(&format!("collection={NS}.{collection}")),
            "{scope}"
        );
    }
}

#[tokio::test]
async fn keeps_an_inbox_scope_for_roles_that_may_invite() {
    let h = start(&[], false).await;
    let (did, _, _) = group(&h).await;
    define(&h, &did, Grant::with("host", &["invite"], &[], &[])).await;
    let any = format!("space:{NS}.inbox?collection={NS}.invite&action=create");
    let bobs = format!("space:{NS}.inbox?authority=did:plc:bob&collection={NS}.invite");
    let other = format!("space:{NS}.inbox?collection={NS}.rule");
    let wanted = [any.as_str(), bobs.as_str(), other.as_str(), "blob:image/*"];
    let narrow = |r: &[&str]| {
        let wanted = asked(&wanted);
        h.host
            .narrow(&did, &roles(r), &wanted, &Default::default())
            .unwrap()
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
    let scope = concrete(
        &asked(&["include:org.atgrp.authManageAdmin"]),
        did,
        &Default::default(),
    );
    let scopes: Vec<&str> = scope.split(' ').collect();
    let all = "action=read&action=create&action=update&action=delete";
    let at = format!("authority={did}&skey=self");
    // Writing records and changing the space are scopes of their own.
    assert_eq!(
        scopes,
        [
            format!(
                "space:{NS}.meta?{at}&collection={NS}.about&collection={NS}.config&collection=com.atproto.label.record&{all}"
            ),
            format!("space:{NS}.meta?{at}&action=read&manage=update"),
            format!(
                "space:{NS}.members?{at}&collection={NS}.rule&collection={NS}.labeler&collection={NS}.role&collection={NS}.membership&collection=com.atproto.label.record&{all}"
            ),
            format!("space:{NS}.members?{at}&action=read&manage=update"),
            format!(
                "space:{NS}.inbox?authority=*&skey=self&collection={NS}.invite&action=read_self&action=create&action=update&action=delete"
            ),
            format!("rpc:{NS}.listJoinRequests?aud=*"),
            format!("rpc:{NS}.denyJoinRequest?aud=*"),
            format!("rpc:{INTERNAL}.listInvites?aud=*"),
            format!("rpc:{NS}.mod.querySubjects?aud=*"),
            format!("rpc:{NS}.mod.querySubjectHistory?aud=*"),
            format!("rpc:{INTERNAL}.getModSubjectBlob?aud=*"),
            format!("rpc:{NS}.mod.updateSubject?aud=*"),
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
    let space = SpaceRef::parse("at:space/did:plc:g/com.example.calendar/rides").unwrap();
    let at = "space:com.example.calendar";
    let policy = json!({"grants": [
        {"role": "a", "permissions": [format!("{at}?collection=x&action=create"), format!("{at}?collection=y")]},
        {"role": "b", "permissions": [format!("{at}?collection=x&action=delete&action=other")]},
        {"role": "c", "permissions": [format!("{at}?collection=z")]},
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
