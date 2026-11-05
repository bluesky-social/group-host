//! Who is asking, and the groups someone is in.

mod common;

use common::groups::*;
use common::oauth::give_avatar;
use common::spaces::*;
use common::{Running, start};
use group_host::group::members_space;
use group_host::repo::Write;
use serde_json::{Value as Json, json};

async fn join(h: &Running, did: &str, who: &str) {
    let value = json!({"$type": format!("{NS}.membership"), "member": who, "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"});
    let write = Write::Create {
        collection: format!("{NS}.membership"),
        rkey: who.into(),
        value,
    };
    h.host
        .space_write(did, &members_space(did), &[write])
        .await
        .unwrap();
}

async fn viewer(h: &Running, who: Option<&Authority>) -> Json {
    let lxm = format!("{NS}.getViewer");
    let headers: Vec<(&str, String)> = who.map(|w| auth(h, w, &lxm)).into_iter().collect();
    let (status, said) = get(h, &lxm, "", &headers).await;
    assert_eq!(status, 200, "{said}");
    said
}

async fn groups_of(h: &Running, who: &Authority, query: &str) -> (u16, Json) {
    let lxm = format!("{NS}.getActorGroups");
    get(h, &lxm, query, &[auth(h, who, &lxm)]).await
}

fn dids(said: &Json) -> Vec<&str> {
    said["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["did"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn says_who_is_asking() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    // No one, and where someone with no account makes one.
    let signup = json!("https://bsky.social");
    assert_eq!(viewer(&h, None).await, json!({"signUpAt": signup}));
    let said = viewer(&h, Some(&alice)).await;
    assert_eq!(said["actor"]["did"], alice.did.as_str());
    assert_eq!(said["actor"]["handle"], "authority.test");
    assert_eq!(said["actor"]["canCreateGroup"], true);
    assert_eq!(said["signUpAt"], signup);
    let h = start(&[("GROUP_FOUNDERS", "did:plc:someone")], false).await;
    let [alice, ..] = people(&h).await;
    assert_eq!(
        viewer(&h, Some(&alice)).await["actor"]["canCreateGroup"],
        false
    );
}

#[tokio::test]
async fn lists_the_groups_someone_is_in_that_the_caller_is_in_too() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let riders = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let hikers = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let quiet = start_group(&h, &alice, json!({"handle": "quiet.groups.test", "displayName": "Quiet", "metaReadableBy": [{"role": "admin"}]})).await;
    join(&h, &riders, &bob.did).await;
    join(&h, &quiet, &bob.did).await;

    let (status, mine) = groups_of(&h, &alice, &format!("actor={}", alice.did)).await;
    assert_eq!(status, 200, "{mine}");
    let mut all = vec![riders.as_str(), hikers.as_str(), quiet.as_str()];
    all.sort();
    assert_eq!(dids(&mine), all);
    let at_riders = mine["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["did"] == riders.as_str())
        .unwrap();
    assert_eq!(at_riders["handle"], RIDERS);
    assert_eq!(at_riders["displayName"], "Riders");
    assert_eq!(at_riders["memberCount"], 2);
    assert!(at_riders["address"].is_string());

    // Of Bob's, only those Alice is in too; of Alice's, only those Bob is.
    let (_, bobs) = groups_of(&h, &alice, &format!("actor={}", bob.did)).await;
    let mut both = vec![riders.as_str(), quiet.as_str()];
    both.sort();
    assert_eq!(dids(&bobs), both);
    let (_, seen) = groups_of(&h, &bob, &format!("actor={}", alice.did)).await;
    assert_eq!(dids(&seen), both);
    // Whose profile Bob may not read shows him no name.
    let at_quiet = seen["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["did"] == quiet.as_str())
        .unwrap();
    assert_eq!(at_quiet.get("displayName"), None);
    let (_, none) = groups_of(&h, &carol, &format!("actor={}", alice.did)).await;
    assert_eq!(none["groups"], json!([]));

    // A page at a time.
    let (_, first) = groups_of(&h, &alice, &format!("actor={}&limit=2", alice.did)).await;
    assert_eq!(dids(&first), all[..2]);
    let cursor = first["cursor"].as_str().unwrap();
    let (_, rest) = groups_of(
        &h,
        &alice,
        &format!("actor={}&limit=2&cursor={cursor}", alice.did),
    )
    .await;
    assert_eq!(dids(&rest), all[2..]);
    assert_eq!(rest.get("cursor"), None);

    assert_eq!(
        groups_of(&h, &alice, "actor=nobody.invalid").await.1["error"],
        "ActorNotFound"
    );
    assert_eq!(groups_of(&h, &alice, "").await.0, 400);
    let lxm = format!("{NS}.getActorGroups");
    assert_eq!(
        get(&h, &lxm, &format!("actor={}", alice.did), &[]).await.0,
        401
    );
}

#[tokio::test]
async fn shows_a_groups_picture() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let riders = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let avatar = give_avatar(&h, &alice, &riders, "Riders", b"face").await;
    let (_, mine) = groups_of(&h, &alice, &format!("actor={}", alice.did)).await;
    assert_eq!(mine["groups"][0]["avatar"], avatar["ref"]["$link"]);
}

#[tokio::test]
async fn names_by_did_alone_someone_whose_document_it_cannot_read() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let riders = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    join(&h, &riders, "did:plc:nobodynobodynobodyxx").await;
    let lxm = format!("{NS}.listMembers");
    let (_, view) = get(
        &h,
        &lxm,
        &format!("group={riders}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    let theirs = view["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["actor"]["did"] == "did:plc:nobodynobodynobodyxx")
        .unwrap();
    assert_eq!(
        theirs["actor"],
        json!({"did": "did:plc:nobodynobodynobodyxx"})
    );
}

#[tokio::test]
async fn names_a_person_the_host_shows() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let lxm = format!("{NS}.getActor");
    let actor = async |asker: &Authority, who: &str| {
        get(&h, &lxm, &format!("actor={who}"), &[auth(&h, asker, &lxm)]).await
    };
    // Oneself, and whether the host starts groups for them.
    let (status, me) = actor(&alice, &alice.did).await;
    assert_eq!(status, 200, "{me}");
    assert_eq!(me["did"], alice.did.as_str());
    assert_eq!(me["handle"], "authority.test");
    assert_eq!(me["canCreateGroup"], true);
    // No one else, until they are in a group here, or asking to join one.
    for who in [&bob, &carol] {
        let said = actor(&alice, &who.did).await;
        assert_eq!(
            (said.0, said.1["error"].as_str()),
            (400, Some("ActorNotFound"))
        );
    }
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    join(&h, &group, &bob.did).await;
    let ask = format!("{NS}.requestJoin");
    let (status, said) = post(
        &h,
        &ask,
        &json!({"group": group}),
        &[auth(&h, &carol, &ask)],
    )
    .await;
    assert_eq!(status, 200, "{said}");
    for who in [&bob, &carol] {
        let (status, view) = actor(&alice, &who.did).await;
        assert_eq!(status, 200, "{view}");
        assert_eq!(view["did"], who.did.as_str());
        assert!(view.get("canCreateGroup").is_none());
    }
    // Asking to join a group switched off is asking nowhere it shows.
    h.host.deactivate_account(&group, &alice.did, &[]).unwrap();
    let said = actor(&alice, &carol.did).await;
    assert_eq!(said.1["error"], "ActorNotFound");
    let said = get(&h, &lxm, &format!("actor={}", bob.did), &[]).await;
    assert_eq!(said.0, 401);

    let closed = start(&[("GROUP_FOUNDERS", "did:plc:someone")], false).await;
    let [dana, ..] = people(&closed).await;
    let (_, me) = get(
        &closed,
        &lxm,
        &format!("actor={}", dana.did),
        &[auth(&closed, &dana, &lxm)],
    )
    .await;
    assert_eq!(me["canCreateGroup"], false);
}
