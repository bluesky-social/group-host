//! What a token signing in as a group may do: the scope asked for,
//! narrowed to the approving member's roles, and made concrete.

mod common;

use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::members_space;
use group_host::oauth_scope::{SpaceScope, concrete};
use group_host::rbac::{Access, Grant};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

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

async fn write(h: &Running, did: &str, space: &str, collection: &str, rkey: &str, value: Json) {
    let write = Write::Update {
        collection: collection.into(),
        rkey: rkey.into(),
        value,
    };
    h.host.space_write(did, space, &[write]).await.unwrap();
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
    let mut permissions = h
        .host
        .group_record(
            &did,
            &members_space(&did),
            &format!("{NS}.permissions"),
            "self",
        )
        .unwrap()
        .unwrap();
    permissions["grants"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role": "poster", "scopes": ["repo:app.bsky.feed.post"], "assignable": []}));
    write(
        h,
        &did,
        &members_space(&did),
        &format!("{NS}.permissions"),
        "self",
        permissions,
    )
    .await;
    (did, made["space"].as_str().unwrap().to_string(), alice)
}

#[tokio::test]
async fn keeps_for_each_role_what_its_grants_hold() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    let all = asked(&[
        "atproto",
        "include:fyi.opensocial.configure",
        "include:com.example.other",
        "repo:app.bsky.feed.post?action=create",
        "repo:app.bsky.feed.like",
        "rpc:app.bsky.feed.getTimeline?aud=*",
        "rpc:fyi.opensocial.listSubjects?aud=*",
        "rpc:*?aud=*",
        "blob:*/*",
        "transition:generic",
    ]);
    let admin = h
        .host
        .narrow(&did, &roles(&["admin"]), &alice.did, &all)
        .unwrap();
    assert_eq!(
        admin,
        asked(&[
            "atproto",
            "include:fyi.opensocial.configure",
            "repo:app.bsky.feed.post?action=create",
            "repo:app.bsky.feed.like",
            "rpc:app.bsky.feed.getTimeline?aud=*",
            "blob:*/*",
        ])
    );
    let poster = h
        .host
        .narrow(&did, &roles(&["poster"]), &alice.did, &all)
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
        .narrow(&did, &roles(&["member"]), &alice.did, &all)
        .unwrap();
    assert_eq!(member, asked(&["atproto"]));
}

#[tokio::test]
async fn reads_every_form_a_scope_may_take() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
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
        .narrow(&did, &roles(&["admin"]), &alice.did, &all)
        .unwrap();
    assert_eq!(admin, all);
    // Kept only when every collection it names is one the roles write.
    let poster = h
        .host
        .narrow(&did, &roles(&["poster"]), &alice.did, &all)
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
async fn keeps_a_space_scope_a_spaces_access_record_grants() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    let member = |scopes: &[&str]| {
        h.host
            .narrow(&did, &roles(&["member"]), &alice.did, &asked(scopes))
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
            &alice.did,
            &asked(&[&format!("space:{CALENDAR}")]),
        )
        .unwrap();
    assert_eq!(admin, [format!("space:{CALENDAR}")]);
}

#[tokio::test]
async fn makes_a_scope_for_every_type_one_for_each_space_the_roles_read() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    // The group's own spaces are among them, read by members.
    let all = h
        .host
        .narrow(
            &did,
            &roles(&["member"]),
            &alice.did,
            &asked(&["space:*?action=read"]),
        )
        .unwrap();
    assert_eq!(
        all,
        [
            format!("space:fyi.opensocial.meta?authority={did}&skey=self&action=read"),
            format!("space:fyi.opensocial.members?authority={did}&skey=self&action=read"),
            format!("space:{CALENDAR}?authority={did}&skey=rides&action=read"),
        ]
    );
    let narrow = |r: &[&str], scope: &str| {
        calendar(
            h.host
                .narrow(&did, &roles(r), &alice.did, &asked(&[scope]))
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
async fn in_a_space_for_chosen_people_reads_for_them_and_writes_for_those_who_may_do_anything() {
    let h = start(&[], false).await;
    let (did, uri, alice) = group(&h).await;
    let access = json!({
        "$type": format!("{NS}.access"),
        "grants": [],
        "people": [{"did": alice.did}],
    });
    write(&h, &did, &uri, &format!("{NS}.access"), "self", access).await;
    let narrow = |r: &[&str], who: &str, scope: &str| {
        calendar(
            h.host
                .narrow(&did, &roles(r), who, &asked(&[scope]))
                .unwrap(),
        )
    };
    let at = format!("space:{CALENDAR}?authority={did}&skey=rides");
    assert_eq!(
        narrow(&["admin"], &alice.did, "space:*"),
        [format!(
            "{at}&collection=*&action=read&action=create&action=update&action=delete"
        )]
    );
    assert_eq!(
        narrow(&["member"], &alice.did, "space:*?action=read"),
        [format!("{at}&action=read")]
    );
    assert_eq!(
        narrow(&["member"], &alice.did, "space:*"),
        Vec::<String>::new()
    );
    assert_eq!(
        narrow(&["admin"], "did:plc:someone", "space:*"),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn gives_nothing_to_write_without_a_permissions_record() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    let gone = Write::Delete {
        collection: format!("{NS}.permissions"),
        rkey: "self".into(),
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
            &alice.did,
            &asked(&["atproto", "include:fyi.opensocial.configure", "repo:x"]),
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
            "include:fyi.opensocial.configure",
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
            "space:fyi.opensocial.meta?authority=did:plc:riders&skey=self&collection=fyi.opensocial.profile&collection=fyi.opensocial.rule&collection=fyi.opensocial.labeler&action=read&action=create&action=update&action=delete",
            "space:fyi.opensocial.members?authority=did:plc:riders&skey=self&collection=fyi.opensocial.role&collection=fyi.opensocial.permissions&action=read&action=create&action=update&action=delete",
            "space:com.example.calendar?authority=did:plc:riders&skey=rides&collection=*",
            "space:com.example.calendar?collection=*",
            "space:com.example.calendar?action=read",
        ]
    );
}

#[tokio::test]
async fn keeps_a_permission_set_for_roles_holding_all_it_covers() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    let space = members_space(&did);
    let collection = format!("{NS}.permissions");
    let found = h.host.group_record(&did, &space, &collection, "self");
    let mut permissions = found.unwrap().unwrap();
    let grants = permissions["grants"].as_array_mut().unwrap();
    for (role, actions) in [
        ("moderator", &["mod.read", "mod.resolve", "label"][..]),
        ("doorkeeper", &["admit", "eject", "role.assign"][..]),
        ("steward", &["space.configure"][..]),
    ] {
        grants.push(Grant::with(role, actions, &[], &[]).to_json());
    }
    write(&h, &did, &space, &collection, "self", permissions).await;
    let sets = asked(&[
        "include:fyi.opensocial.configure",
        "include:fyi.opensocial.manageMembers",
        "include:fyi.opensocial.manageSpaces",
        "include:fyi.opensocial.moderate",
        // For a person's own session, never a group's.
        "include:fyi.opensocial.participate",
        // Not a permission set.
        "include:fyi.opensocial.profile",
        "blob:*/*",
    ]);
    let narrow = |r: &[&str]| h.host.narrow(&did, &roles(r), &alice.did, &sets).unwrap();
    assert_eq!(narrow(&["admin"]), {
        let mut all = sets[..4].to_vec();
        all.push("blob:*/*".into());
        all
    });
    assert_eq!(
        narrow(&["moderator"]),
        asked(&["include:fyi.opensocial.moderate", "blob:*/*"])
    );
    assert_eq!(
        narrow(&["steward"]),
        asked(&["include:fyi.opensocial.manageSpaces", "blob:*/*"])
    );
    // Admitting and ejecting without inviting is not all of managing members.
    assert_eq!(narrow(&["doorkeeper"]), Vec::<String>::new());
    assert_eq!(narrow(&["member"]), Vec::<String>::new());
}

#[tokio::test]
async fn keeps_an_inbox_scope_for_roles_that_may_invite() {
    let h = start(&[], false).await;
    let (did, _, alice) = group(&h).await;
    let space = members_space(&did);
    let collection = format!("{NS}.permissions");
    let found = h.host.group_record(&did, &space, &collection, "self");
    let mut permissions = found.unwrap().unwrap();
    let grants = permissions["grants"].as_array_mut().unwrap();
    grants.push(Grant::with("host", &["invite"], &[], &[]).to_json());
    write(&h, &did, &space, &collection, "self", permissions).await;
    let any = format!("space:{NS}.invites?collection={NS}.invite&action=create");
    let bobs = format!("space:{NS}.invites?authority=did:plc:bob&collection={NS}.invite");
    let other = format!("space:{NS}.invites?collection={NS}.rule");
    let wanted = [any.as_str(), bobs.as_str(), other.as_str(), "blob:image/*"];
    let narrow = |r: &[&str]| {
        let wanted = asked(&wanted);
        h.host.narrow(&did, &roles(r), &alice.did, &wanted).unwrap()
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
        &asked(&[
            "include:fyi.opensocial.manageMembers",
            "include:fyi.opensocial.manageSpaces",
            "include:fyi.opensocial.moderate",
        ]),
        did,
    );
    let scopes: Vec<&str> = scope.split(' ').collect();
    let all = "action=read&action=create&action=update&action=delete";
    assert_eq!(
        scopes,
        [
            format!("space:fyi.opensocial.members?authority={did}&skey=self&collection=fyi.opensocial.membership&{all}"),
            "space:fyi.opensocial.invites?authority=*&skey=self&collection=fyi.opensocial.invite&action=read_self&action=create&action=update&action=delete".into(),
            "rpc:fyi.opensocial.listJoinRequests?aud=*".into(),
            "rpc:fyi.opensocial.denyJoinRequest?aud=*".into(),
            "rpc:fyi.opensocial.listInvites?aud=*".into(),
            format!("space:fyi.opensocial.meta?authority={did}&skey=self&collection=fyi.opensocial.access&{all}"),
            format!("space:fyi.opensocial.members?authority={did}&skey=self&collection=fyi.opensocial.space&collection=fyi.opensocial.access&{all}"),
            format!("space:fyi.opensocial.meta?authority={did}&skey=self&collection=com.atproto.label.record&{all}"),
            format!("space:fyi.opensocial.members?authority={did}&skey=self&collection=com.atproto.label.record&{all}"),
            "rpc:fyi.opensocial.listSubjects?aud=*".into(),
            "rpc:fyi.opensocial.getSubjectHistory?aud=*".into(),
            "rpc:fyi.opensocial.resolveSubject?aud=*".into(),
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
    let space = "at://did:plc:g/space/com.example.calendar/rides";
    let at = "space:com.example.calendar";
    let access = json!({"grants": [
        {"role": "a", "scopes": [format!("{at}?collection=x&action=create"), format!("{at}?collection=y")]},
        {"role": "b", "scopes": [format!("{at}?collection=x&action=delete&action=other")]},
        {"role": "c", "scopes": [format!("{at}?collection=z")]},
    ]});
    let writes = Access::of(&access, space).writes(&roles(&["a", "b"]));
    let x: Vec<&str> = writes["x"].iter().map(String::as_str).collect();
    assert_eq!(x, ["create", "delete"]);
    assert_eq!(writes["y"].len(), 3);
    assert!(!writes.contains_key("z"));
    assert!(
        Access::of(&json!({}), space)
            .writes(&roles(&["a"]))
            .is_empty()
    );
}
