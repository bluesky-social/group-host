//! What a group's records let each of its people do.

use group_host::group::join_policy;
use group_host::rbac::{
    ACTIONS, Access, Grant, actions_for, actions_of, grants, listed, repo_collections_for, scope_of,
};
use serde_json::json;

fn roles(names: &[&str]) -> Vec<String> {
    names.iter().map(|r| r.to_string()).collect()
}

const CALENDAR: &str = "at://did:plc:g/space/xyz.app.calendar/rides";

#[test]
fn reads_and_writes_by_the_access_record() {
    let read = "space:xyz.app.calendar?action=read";
    let members = Access::of(
        &json!({"grants": [{"role": "member", "scopes": [read]}, {"role": "guest", "scopes": []}]}),
        CALENDAR,
    );
    assert!(members.can_read(&roles(&["member"]), "did:plc:a"));
    assert!(!members.can_read(&[], "did:plc:a"));
    assert!(!members.can_read(&roles(&["guest"]), "did:plc:a"));
    // A member whose roles grant read writes, unless it says who.
    assert!(members.can_write_own(&roles(&["member"]), "did:plc:a"));
    assert!(!members.can_write_own(&roles(&["guest"]), "did:plc:a"));
    assert!(!members.can_write_own(&[], "did:plc:a"));
    // Public: anyone signed in reads; members granted nothing write nothing.
    let public = Access::of(&json!({"public": true, "grants": []}), CALENDAR);
    assert!(public.can_read(&[], "did:plc:a"));
    assert!(!public.can_write_own(&roles(&["member"]), "did:plc:a"));
    let open = Access::of(
        &json!({"public": true, "grants": [], "writableBy": [{"role": "public"}]}),
        CALENDAR,
    );
    assert!(open.can_write_own(&[], "did:plc:a"));
    let mods = Access::of(
        &json!({"grants": [{"role": "member", "scopes": [read]}], "writableBy": [{"role": "mod"}]}),
        CALENDAR,
    );
    assert!(mods.can_write_own(&roles(&["mod"]), "did:plc:a"));
    assert!(!mods.can_write_own(&roles(&["member"]), "did:plc:a"));

    // A grant counts in its own space: read of another type, or of
    // another skey, is no read here, and nothing counts where no space is.
    for elsewhere in [
        "space:xyz.app.forum?action=read",
        "space:xyz.app.calendar?skey=laps&action=read",
        "space:xyz.app.calendar?authority=did:plc:other&action=read",
        "repo:xyz.event",
    ] {
        let access = Access::of(
            &json!({"grants": [{"role": "member", "scopes": [elsewhere]}]}),
            CALENDAR,
        );
        assert!(
            !access.can_read(&roles(&["member"]), "did:plc:a"),
            "{elsewhere}"
        );
    }
    for here in [
        "space:*?action=read",
        "space:xyz.app.calendar?skey=rides",
        "space:xyz.app.calendar?authority=did:plc:g&action=read",
    ] {
        let access = Access::of(
            &json!({"grants": [{"role": "member", "scopes": [here]}]}),
            CALENDAR,
        );
        assert!(access.can_read(&roles(&["member"]), "did:plc:a"), "{here}");
    }
    let nowhere = Access::of(
        &json!({"grants": [{"role": "member", "scopes": [read]}]}),
        "nowhere",
    );
    assert!(!nowhere.can_read(&roles(&["member"]), "did:plc:a"));

    // What a role writes as the group: each collection a scope names, as
    // its actions say, every write when it names none.
    let writers = Access::of(
        &json!({"grants": [
            {"role": "admin", "scopes": ["space:xyz.app.calendar?collection=xyz.event&collection=xyz.rsvp", "space:xyz.app.forum?collection=xyz.post"]},
            {"role": "member", "scopes": ["space:xyz.app.calendar?collection=xyz.rsvp&action=create", read]},
        ]}),
        CALENDAR,
    );
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
    assert!(writers.can_read(&roles(&["admin"]), "did:plc:b"));

    // For chosen people: them while they are members, and the roles
    // granted read.
    let chosen = Access::of(
        &json!({
            "public": true,
            "grants": [{"role": "admin", "scopes": [read]}],
            "people": [{"did": "did:plc:a"}],
        }),
        CALENDAR,
    );
    assert!(chosen.can_read(&roles(&["member"]), "did:plc:a"));
    assert!(!chosen.can_read(&[], "did:plc:a"));
    assert!(!chosen.can_read(&roles(&["member"]), "did:plc:b"));
    assert!(chosen.can_read(&roles(&["admin"]), "did:plc:b"));
    assert!(chosen.can_write_own(&roles(&["member"]), "did:plc:a"));
    assert!(!chosen.can_write_own(&roles(&["admin"]), "did:plc:b"));
    assert_eq!(Access::of(&json!({}), "nowhere"), Access::default());
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
    ] {
        assert_eq!(
            actions_of(&[nothing.into()])
                .into_iter()
                .collect::<Vec<_>>(),
            none,
            "{nothing}"
        );
    }
    // A space scope over every collection labels, takes down and
    // configures; one managing makes and deletes spaces.
    let every = actions_of(&["space:*".into()]);
    assert_eq!(
        every.into_iter().collect::<Vec<_>>(),
        ["label", "space.configure", "takedown"]
    );
    let star = actions_of(&["space:*?collection=*".into()]);
    assert_eq!(star.len(), 3);
    let manage = actions_of(&["space:*?manage=create&manage=delete".into()]);
    assert_eq!(
        manage.into_iter().collect::<Vec<_>>(),
        ["space.create", "space.delete"]
    );
    let other = actions_of(&["space:*?collection=xyz.task".into()]);
    assert!(other.is_empty());
}

#[test]
fn roles_compose_by_union() {
    let record = json!({"grants": [
        Grant::with("admin", &["invite", "admit"], &["member"], &["*"]).to_json(),
        Grant::with("mod", &["label"], &[], &["xyz.post?action=create"]).to_json(),
        {"role": "odd"},
    ]});
    let granted = grants(&record);
    assert_eq!(granted.len(), 3);
    assert_eq!(granted[0].assignable, ["member"]);
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
    assert!(grants(&json!({})).is_empty());
}

#[test]
fn reads_a_join_policy_in_either_words() {
    assert_eq!(join_policy(None).unwrap(), "approval");
    assert_eq!(
        join_policy(Some("fyi.opensocial.profile#request")).unwrap(),
        "approval"
    );
    assert_eq!(join_policy(Some("approval")).unwrap(), "approval");
    assert_eq!(
        join_policy(Some("fyi.opensocial.profile#open")).unwrap(),
        "open"
    );
    assert_eq!(join_policy(Some("invite")).unwrap(), "invite");
    assert_eq!(
        join_policy(Some("sometimes")).unwrap_err().message,
        "joinPolicy is open, approval or invite, not sometimes"
    );
}
