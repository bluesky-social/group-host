//! What a group's records let each of its people do.

use atproto::space::SpaceRef;
use group_host::group::{did_rkey, join_policy, rkey_did};
use group_host::rbac::{
    ACTIONS, Grant, Policy, actions_for, actions_of, assigns, grants, listed, repo_collections_for,
    scope_of,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn roles(names: &[&str]) -> Vec<String> {
    names.iter().map(|r| r.to_string()).collect()
}

const CALENDAR: &str = "at://did:plc:g/space/xyz.app.calendar/rides";

fn policy(value: serde_json::Value) -> Policy {
    Policy::of(&value, &SpaceRef::parse(CALENDAR).unwrap())
}

#[test]
fn reads_writes_and_manages_by_the_policy() {
    let members = policy(json!({"grants": [
        {"role": "member", "read": true},
        {"role": "mod", "read": true, "write": true},
        {"role": "guest"},
    ]}));
    assert!(members.can_read(&roles(&["member"])));
    assert!(!members.can_read(&[]));
    assert!(!members.can_read(&roles(&["guest"])));
    // Reading is not writing: only roles granted `write` write.
    assert!(!members.writes_own(&roles(&["member"])));
    assert!(members.writes_own(&roles(&["member", "mod"])));
    assert!(!members.writes_own(&[]));
    // publicRead: anyone reads; nobody writes for it.
    let public = policy(json!({"publicRead": true, "unlisted": true, "grants": []}));
    assert!(public.can_read(&[]));
    assert!(public.unlisted);
    assert!(!public.writes_own(&roles(&["member"])));
    assert_eq!(members.grant("mod").map(|g| g.write), Some(true));
    assert_eq!(members.grant("admin"), None);

    // Scopes act as the group, and count only here: a scope for another
    // type, skey or authority manages nothing here.
    for elsewhere in [
        "space:xyz.app.forum?manage=update",
        "space:xyz.app.calendar?skey=laps&manage=update",
        "space:xyz.app.calendar?authority=did:plc:other&manage=update",
        "repo:xyz.event",
    ] {
        let p = policy(json!({"grants": [{"role": "member", "scopes": [elsewhere]}]}));
        assert!(!p.manages(&roles(&["member"]), "update"), "{elsewhere}");
    }
    for here in [
        "space:*?manage=update",
        "space:xyz.app.calendar?skey=rides&manage=update&manage=delete",
        "space:xyz.app.calendar?authority=did:plc:g&manage=update",
    ] {
        let p = policy(json!({"grants": [{"role": "member", "scopes": [here]}]}));
        assert!(p.manages(&roles(&["member"]), "update"), "{here}");
        assert!(!p.manages(&roles(&["guest"]), "update"), "{here}");
    }

    // What a role writes as the group: each collection a scope names, as
    // its actions say, every write when it names none.
    let writers = policy(json!({"grants": [
        {"role": "admin", "scopes": ["space:xyz.app.calendar?collection=xyz.event&collection=xyz.rsvp", "space:xyz.app.forum?collection=xyz.post"]},
        {"role": "member", "read": true, "scopes": ["space:xyz.app.calendar?collection=xyz.rsvp&action=create"]},
    ]}));
    let writes = writers.writes(&roles(&["admin", "member"]));
    let as_lists: Vec<(&str, Vec<&str>)> = writes
        .iter()
        .map(|(c, a)| (c.as_str(), a.iter().map(String::as_str).collect()))
        .collect();
    assert_eq!(
        as_lists,
        [
            ("xyz.event", vec!["create", "delete", "update"]),
            ("xyz.rsvp", vec!["create", "delete", "update"]),
        ]
    );
    let member = writers.writes(&roles(&["member"]));
    assert_eq!(member["xyz.rsvp"].len(), 1);
    assert_eq!(writers.scopes(&roles(&["member"])).len(), 1);
    assert_eq!(
        listed(&json!({"x": [{"role": 1}, {"role": "a"}]}), "x", "role"),
        ["a"]
    );
}

#[test]
fn answers_each_action_by_its_scope() {
    for action in ACTIONS {
        let scope = scope_of(action).unwrap();
        assert!(
            actions_of(std::slice::from_ref(&scope)).contains(action),
            "{action} by {scope}"
        );
    }
    assert_eq!(scope_of("fly"), None);
    let none: [&str; 0] = [];
    for nothing in [
        "rpc:fyi.opensocial.fly?aud=*",
        "rpc:com.example.thing",
        "repo:*",
        "blob:*/*",
        // An inbox scope for anything but invites.
        "space:fyi.opensocial.invites?collection=fyi.opensocial.rule",
    ] {
        assert_eq!(
            actions_of(&[nothing.into()])
                .into_iter()
                .collect::<Vec<_>>(),
            none,
            "{nothing}"
        );
    }
    // A space scope over every collection labels and writes memberships;
    // one managing makes, changes and deletes spaces.
    let every = actions_of(&["space:*".into()]);
    assert_eq!(
        every.into_iter().collect::<Vec<_>>(),
        ["admit", "eject", "label", "role.assign"]
    );
    let star = actions_of(&["space:*?collection=*".into()]);
    assert_eq!(star.len(), 4);
    assert_eq!(
        of_scope("space:*?manage=update&manage=delete"),
        ["space.configure", "space.delete"]
    );
    // Labels, written in any of the group's spaces, label.
    assert_eq!(
        of_scope("space:*?collection=com.atproto.label.record"),
        ["label"]
    );
    // Memberships written in the members space admit, give roles and
    // put out, as far as the scope's writes go; the set for running
    // members does all of that, and invites.
    let of = of_scope;
    let members = "space:fyi.opensocial.members?collection=fyi.opensocial.membership";
    assert_eq!(of(members), ["admit", "eject", "role.assign"]);
    assert_eq!(of(&format!("{members}&action=update")), ["role.assign"]);
    assert_eq!(
        of("space:fyi.opensocial.members?collection=fyi.opensocial.role"),
        none
    );
    assert_eq!(
        of("space:fyi.opensocial.meta?collection=fyi.opensocial.membership"),
        none
    );
    assert_eq!(of("space:fyi.opensocial.members?manage=create"), none);
    // The tiered sets: moderating gives memberships, invites and the
    // moderation methods, and labels in the meta and members spaces alone,
    // not `label`, which is labels in every space; administering, those
    // and configuring; the profile, nothing, as the meta space's policy
    // says who writes it.
    let moderates = [
        "admit",
        "eject",
        "invite",
        "mod.read",
        "mod.resolve",
        "role.assign",
    ];
    assert_eq!(of("include:fyi.opensocial.authModerate"), moderates);
    assert_eq!(
        of("include:fyi.opensocial.authManageGroup"),
        [&["admit", "eject", "group.configure"][..], &moderates[2..]].concat()
    );
    assert_eq!(of("include:fyi.opensocial.authManageProfile"), none);
    let manage = actions_of(&["space:*?manage=create&manage=delete".into()]);
    assert_eq!(
        manage.into_iter().collect::<Vec<_>>(),
        ["space.create", "space.delete"]
    );
    let other = actions_of(&["space:*?collection=xyz.task".into()]);
    assert!(other.is_empty());
}

fn of_scope(scope: &str) -> Vec<&'static str> {
    actions_of(&[scope.into()]).into_iter().collect()
}

#[test]
fn roles_compose_by_union() {
    let admin = Grant::with("admin", &["invite", "admit"], &["member"], &["*"]);
    let record =
        |g: &Grant| json!({"displayName": g.role, "scopes": g.scopes, "assignable": g.assignable});
    let records: Vec<(String, Value)> = vec![
        ("admin".into(), record(&admin)),
        (
            "mod".into(),
            record(&Grant::with(
                "mod",
                &["label"],
                &[],
                &["xyz.post?action=create"],
            )),
        ),
        ("odd".into(), json!({"displayName": "Odd"})),
    ];
    let granted = grants(&records);
    assert_eq!(granted.len(), 3);
    assert_eq!(granted[0], admin);
    assert_eq!(granted[2].scopes, Vec::<String>::new());
    let both = roles(&["admin", "mod"]);
    assert_eq!(
        actions_for(&granted, &both).into_iter().collect::<Vec<_>>(),
        ["admit", "invite", "label"]
    );
    assert_eq!(
        repo_collections_for(&granted, &both)
            .into_iter()
            .collect::<Vec<_>>(),
        ["*", "xyz.post"]
    );
    assert!(actions_for(&granted, &[]).is_empty());
    assert!(grants(&[]).is_empty());
}

#[test]
fn every_role_is_assigned_by_a_star() {
    let named: BTreeSet<String> = ["member".to_string()].into();
    assert!(assigns(&named, "member"));
    assert!(!assigns(&named, "later"));
    let every: BTreeSet<String> = ["*".to_string()].into();
    assert!(assigns(&every, "member"));
    assert!(assigns(&every, "later"));
}

#[test]
fn keys_a_membership_by_its_member_s_did() {
    for did in [
        "did:plc:abc",
        "did:web:example.com",
        "did:web:localhost%3A2583",
        "did:web:localhost%3A2583:users:alice",
    ] {
        assert_eq!(rkey_did(&did_rkey(did)), did);
    }
    // A path's first part that is not a port stays one.
    assert_eq!(
        rkey_did("did:web:example.com:users"),
        "did:web:example.com:users"
    );
    assert_eq!(rkey_did("did:web:example.com:"), "did:web:example.com:");
}

#[test]
fn grants_the_queue_by_the_moderation_methods() {
    let rpc = |m: &str| format!("rpc:fyi.opensocial.{m}?aud=*");
    assert_eq!(scope_of("mod.read").unwrap(), rpc("listModSubjects"));
    assert_eq!(scope_of("mod.resolve").unwrap(), rpc("resolveModSubject"));
    let scopes = [rpc("listModSubjects"), rpc("resolveModSubject")];
    assert_eq!(
        actions_of(&scopes).into_iter().collect::<Vec<_>>(),
        ["mod.read", "mod.resolve"]
    );
}

#[test]
fn reads_a_join_policy_as_the_lexicon_words_it() {
    assert_eq!(join_policy(None).unwrap(), "approval");
    assert_eq!(join_policy(Some("approval")).unwrap(), "approval");
    assert_eq!(join_policy(Some("open")).unwrap(), "open");
    assert_eq!(join_policy(Some("invite")).unwrap(), "invite");
    for other in ["sometimes", "fyi.opensocial.profile#open", "request"] {
        assert_eq!(
            join_policy(Some(other)).unwrap_err().message,
            format!("joinPolicy is open, approval or invite, not {other}")
        );
    }
}
