//! What a group's records let each of its people do.

use group_host::group::join_policy;
use group_host::rbac::{
    ACTIONS, Access, Grant, actions_for, actions_of, grants, listed, repo_collections_for, scope_of,
};
use serde_json::json;

fn roles(names: &[&str]) -> Vec<String> {
    names.iter().map(|r| r.to_string()).collect()
}

#[test]
fn reads_and_writes_by_the_access_record() {
    let members = Access::of(&json!({"readableBy": [{"role": "member"}]}));
    assert!(members.can_read(&roles(&["member"]), "did:plc:a"));
    assert!(!members.can_read(&[], "did:plc:a"));
    // Any member writes, unless it says who.
    assert!(members.can_write_own(&roles(&["member"]), "did:plc:a"));
    assert!(!members.can_write_own(&[], "did:plc:a"));
    let public = Access::of(
        &json!({"readableBy": [{"role": "public"}], "writableBy": [{"role": "public"}]}),
    );
    assert!(public.can_read(&[], "did:plc:a"));
    assert!(public.can_write_own(&[], "did:plc:a"));
    let mods = Access::of(&json!({"readableBy": [], "writableBy": [{"role": "mod"}]}));
    assert!(mods.can_write_own(&roles(&["mod"]), "did:plc:a"));
    assert!(!mods.can_write_own(&roles(&["member"]), "did:plc:a"));

    // For chosen people: them while they are members, and its reviewers.
    let chosen = Access::of(&json!({
        "readableBy": [{"role": "public"}],
        "people": [{"did": "did:plc:a"}],
        "reviewers": [{"role": "admin"}],
    }));
    assert!(chosen.can_read(&roles(&["member"]), "did:plc:a"));
    assert!(!chosen.can_read(&[], "did:plc:a"));
    assert!(!chosen.can_read(&roles(&["member"]), "did:plc:b"));
    assert!(chosen.can_read(&roles(&["admin"]), "did:plc:b"));
    assert!(chosen.can_write_own(&roles(&["member"]), "did:plc:a"));
    assert!(!chosen.can_write_own(&roles(&["admin"]), "did:plc:b"));
    assert_eq!(Access::of(&json!({})), Access::default());
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
