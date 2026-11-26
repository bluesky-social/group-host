//! What a group's records let each of its people do.

use atproto::space::SpaceRef;
use group_host::group::{did_rkey, join_policy, rkey_did};
use group_host::rbac::{
    ACTIONS, Grant, Policy, actions_for, actions_of, assigns, grants, listed,
    members_records_writes, meta_writes, repo_collections_for, scope_of,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn roles(names: &[&str]) -> Vec<String> {
    names.iter().map(|r| r.to_string()).collect()
}

const CALENDAR: &str = "at:space/did:plc:g/xyz.app.calendar/rides";

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

    // Permissions act as the group, and count only here: one for another
    // type, skey or authority manages nothing here.
    for elsewhere in [
        "space:xyz.app.forum?manage=update",
        "space:xyz.app.calendar?skey=laps&manage=update",
        "space:xyz.app.calendar?authority=did:plc:other&manage=update",
        "repo:xyz.event",
    ] {
        let p = policy(json!({"grants": [{"role": "member", "permissions": [elsewhere]}]}));
        assert!(!p.manages(&roles(&["member"]), "update"), "{elsewhere}");
    }
    for here in [
        "space:*?manage=update",
        "space:xyz.app.calendar?skey=rides&manage=update&manage=delete",
        "space:xyz.app.calendar?authority=did:plc:g&manage=update",
    ] {
        let p = policy(json!({"grants": [{"role": "member", "permissions": [here]}]}));
        assert!(p.manages(&roles(&["member"]), "update"), "{here}");
        assert!(!p.manages(&roles(&["guest"]), "update"), "{here}");
    }

    // What a role writes as the group: each collection a permission names,
    // as its actions say, every write when it names none.
    let writers = policy(json!({"grants": [
        {"role": "admin", "permissions": ["space:xyz.app.calendar?collection=xyz.event&collection=xyz.rsvp", "space:xyz.app.forum?collection=xyz.post"]},
        {"role": "member", "read": true, "permissions": ["space:xyz.app.calendar?collection=xyz.rsvp&action=create"]},
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
    assert_eq!(writers.permissions(&roles(&["member"])).len(), 1);
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
        "rpc:org.atgrp.fly?aud=*",
        "rpc:com.example.thing",
        "repo:*",
        "blob:*/*",
        // An inbox scope for anything but invites.
        "space:org.atgrp.inbox?collection=org.atgrp.rule",
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
    let members = "space:org.atgrp.members?collection=org.atgrp.membership";
    assert_eq!(of(members), ["admit", "eject", "role.assign"]);
    assert_eq!(of(&format!("{members}&action=update")), ["role.assign"]);
    assert_eq!(
        of("space:org.atgrp.members?collection=org.atgrp.role"),
        none
    );
    assert_eq!(
        of("space:org.atgrp.meta?collection=org.atgrp.membership"),
        none
    );
    assert_eq!(of("space:org.atgrp.members?manage=create"), none);
    // The group's permission sets, as their lexicons list them: running
    // members admits, gives roles, puts out and invites; moderating puts
    // out and works the moderation methods, its labels the meta and
    // members spaces' alone, not `label`, which is labels in every space;
    // administering, all of those and configuring; the basics, nothing a
    // method asks about.
    assert_eq!(
        of("include:org.atgrp.authManageMembers"),
        ["admit", "eject", "invite", "role.assign"]
    );
    assert_eq!(
        of("include:org.atgrp.authModerate"),
        ["eject", "mod.read", "mod.resolve"]
    );
    assert_eq!(
        of("include:org.atgrp.authManageAdmin"),
        [
            "admit",
            "eject",
            "group.configure",
            "invite",
            "mod.read",
            "mod.resolve",
            "role.assign"
        ]
    );
    assert_eq!(of("include:org.atgrp.authManageBasic"), none);
    assert_eq!(of("include:org.atgrp.authParticipate"), none);
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
    let record = |g: &Grant| json!({"displayName": g.role, "permissions": g.permissions, "canAssign": g.can_assign});
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
    assert_eq!(granted[2].permissions, Vec::<String>::new());
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
    let rpc = |m: &str| format!("rpc:org.atgrp.{m}?aud=*");
    assert_eq!(scope_of("mod.read").unwrap(), rpc("mod.querySubjects"));
    assert_eq!(scope_of("mod.resolve").unwrap(), rpc("mod.updateSubject"));
    let history = [rpc("mod.querySubjectHistory")];
    assert_eq!(
        actions_of(&history).into_iter().collect::<Vec<_>>(),
        ["mod.read"]
    );
    let scopes = [rpc("mod.querySubjects"), rpc("mod.updateSubject")];
    assert_eq!(
        actions_of(&scopes).into_iter().collect::<Vec<_>>(),
        ["mod.read", "mod.resolve"]
    );
}

#[test]
fn writes_the_group_s_own_records_by_its_permission_sets() {
    let group = "did:plc:g";
    let set = |name: &str| format!("include:org.atgrp.{name}");
    let role =
        |key: &str, permissions: &[String]| Grant::of(key, &json!({ "permissions": permissions }));
    let granted = [
        role("basic", &[set("authManageBasic")]),
        role("admin", &[set("authManageAdmin")]),
        role("moderator", &[set("authModerate")]),
        role("everything", &["space:*?collection=*".into()]),
    ];
    let kinds = |writes: std::collections::BTreeMap<String, BTreeSet<String>>| {
        writes.into_keys().collect::<Vec<_>>()
    };
    let meta = |r: &str| kinds(meta_writes(&granted, &roles(&[r]), group));
    let members = |r: &str| kinds(members_records_writes(&granted, &roles(&[r]), group));
    // The basics write the about record and the rules; administering, the
    // config, labeler and roles too; moderating, none of them.
    assert_eq!(meta("basic"), ["org.atgrp.about"]);
    assert_eq!(members("basic"), ["org.atgrp.rule"]);
    assert_eq!(meta("admin"), ["org.atgrp.about", "org.atgrp.config"]);
    assert_eq!(
        members("admin"),
        ["org.atgrp.labeler", "org.atgrp.role", "org.atgrp.rule"]
    );
    assert!(meta("moderator").is_empty());
    assert!(members("moderator").is_empty());
    assert_eq!(meta("everything").len(), 2);
    assert_eq!(members("everything").len(), 3);
    let all = meta_writes(&granted, &roles(&["basic"]), group);
    let about: Vec<&String> = all["org.atgrp.about"].iter().collect();
    assert_eq!(about, ["create", "delete", "update"]);
}

#[test]
fn reads_a_join_policy_as_the_lexicon_words_it() {
    assert_eq!(join_policy(None).unwrap(), "approval");
    assert_eq!(join_policy(Some("approval")).unwrap(), "approval");
    assert_eq!(join_policy(Some("open")).unwrap(), "open");
    assert_eq!(join_policy(Some("invite")).unwrap(), "invite");
    for other in ["sometimes", "org.atgrp.config#open", "request"] {
        assert_eq!(
            join_policy(Some(other)).unwrap_err().message,
            format!("joinPolicy is open, approval or invite, not {other}")
        );
    }
}
