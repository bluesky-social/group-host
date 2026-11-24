//! A group switched off, back on, and deleted: what reads and writes it
//! refuses while it is off, who may switch it back, and what goes when
//! it is deleted.

mod common;

use atproto::plc::Signed;
use common::groups::*;
use common::spaces::*;
use common::{Running, start, turn};
use group_host::authority::Access;
use group_host::group::{members_space, meta_space};
use group_host::repo::Write;
use group_host::store::Registration;
use serde_json::{Value as Json, json};

const NS: &str = "fyi.opensocial";

async fn call(h: &Running, who: &Authority, method: &str, did: &str) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &json!({"group": did}), &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// A group Alice started with Bob beside her, and Carol a plain member.
async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "stewards": [{"did": bob.did}]}),
    )
    .await;
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[Write::Create {
                collection: format!("{NS}.membership"),
                rkey: carol.did.clone(),
                value: json!({"$type": format!("{NS}.membership"), "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    (did, [alice, bob, carol])
}

#[tokio::test]
async fn a_group_switched_off_is_read_by_nobody() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    // A plain member may not switch it off.
    assert_eq!(
        error(&call(&h, &carol, "deactivateGroup", &did).await),
        (403, "Forbidden")
    );
    assert_eq!(
        call(&h, &alice, "deactivateGroup", &did).await,
        (200, json!({}))
    );
    assert_eq!(
        error(&call(&h, &alice, "deactivateGroup", &did).await),
        (400, "AlreadyDeactivated")
    );
    let off = h.host.deactivation(&did).unwrap().unwrap();
    assert_eq!(off.by, alice.did);
    let mut stewards = off.stewards.clone();
    stewards.sort();
    let mut expected = vec![alice.did.clone(), bob.did.clone()];
    expected.sort();
    assert_eq!(stewards, expected);

    // Its repo is read by nobody, and its spaces neither.
    for (nsid, query) in [
        ("com.atproto.sync.getRepo", format!("did={did}")),
        ("com.atproto.sync.getLatestCommit", format!("did={did}")),
        (
            "com.atproto.repo.listRecords",
            format!("repo={did}&collection={NS}.declaration"),
        ),
    ] {
        let (status, said) = get(&h, nsid, &query, &[]).await;
        assert_eq!(
            (status, said["error"].as_str().unwrap()),
            (400, "RepoDeactivated"),
            "{nsid}"
        );
    }
    let (_, repos) = get(&h, "com.atproto.sync.listRepos", "", &[]).await;
    assert_eq!(repos["repos"][0]["active"], false);
    assert_eq!(repos["repos"][0]["status"], "deactivated");
    let members = members_space(&did);
    let hosted = h.host.store.space(&members).unwrap().unwrap();
    assert!(
        !h.host
            .may(&members, &hosted, Some(&alice.did), Access::Read)
            .unwrap()
    );
    // Nothing is written as it.
    let refused = h
        .host
        .space_write(&did, &meta_space(&did), &[])
        .await
        .unwrap_err();
    assert_eq!(refused.error, "RepoDeactivated");
    assert_eq!(
        h.host.write(&did, &[]).unwrap_err().error,
        "RepoDeactivated"
    );
    // Its repo in a space is read by nobody either.
    let key = atproto::Keypair::generate(atproto::Curve::P256);
    let kem = h
        .host
        .store
        .account(&did)
        .unwrap()
        .unwrap()
        .kem
        .public()
        .did_key();
    let credential = {
        let group = h.host.store.account(&did).unwrap().unwrap();
        let make = atproto::space_auth::Make {
            iss: &did,
            sub: &members,
            key_id: Some(&key.public().did_key()),
            ..Default::default()
        };
        atproto::space_auth::create(
            atproto::space_auth::Kind::Credential,
            &make,
            &group.signing,
            now(),
        )
        .unwrap()
    };
    let syncer = Syncer { key, credential };
    let (headers, _) = signed(&syncer, &did, &kem, atproto::space::COMMIT_MAC);
    let got = read(
        &h,
        "com.atproto.space.getLatestCommit",
        &format!("space={members}&repo={did}"),
        &headers,
    )
    .await;
    assert_eq!(got.error(), (400, "RepoDeactivated".into()));

    // Shown as switched off, and nothing more, to its admins too.
    let lxm = format!("{NS}.getGroup");
    let (_, shown) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(shown["group"]["deactivated"], true);
    assert_eq!(shown["group"].get("displayName"), None);
    // Its stewards of then are told they are, to find the switch; a plain
    // member of then is not, nor is anything of their standing read.
    let viewer = json!({"member": false, "roles": [], "actions": [], "steward": true});
    assert_eq!(shown["group"]["viewer"], viewer);
    let (_, shown) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &carol, &lxm)]).await;
    let viewer = json!({"member": false, "roles": [], "actions": [], "steward": false});
    assert_eq!(shown["group"]["viewer"], viewer);
    // To no one signed in, as any other: its DID and handle.
    let (_, shown) = get(&h, &format!("{NS}.getGroup"), &format!("group={did}"), &[]).await;
    assert_eq!(shown["group"].get("deactivated"), None);

    // Off twice, or on when on, changes nothing.
    assert!(!h.host.deactivate_account(&did, &alice.did, &[]).unwrap());
    assert_eq!(h.host.deactivation(&did).unwrap().unwrap().by, alice.did);

    // Back on, by a steward of when it went off.
    let dave = authority(&h, false, true).await;
    assert_eq!(
        error(&call(&h, &dave, "activateGroup", &did).await),
        (403, "Forbidden")
    );
    assert_eq!(
        call(&h, &bob, "activateGroup", &did).await,
        (200, json!({}))
    );
    assert_eq!(
        error(&call(&h, &bob, "activateGroup", &did).await),
        (400, "NotDeactivated")
    );
    assert!(
        h.host
            .may(&members, &hosted, Some(&alice.did), Access::Read)
            .unwrap()
    );
    let (status, _) = get(
        &h,
        "com.atproto.sync.getLatestCommit",
        &format!("did={did}"),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert!(!h.host.activate_account(&did).unwrap());

    // The relays heard of each.
    let events = h.host.store.events_after(0, 1000).unwrap();
    let accounts: Vec<Json> = events
        .iter()
        .filter(|e| e.kind == "#account")
        .map(|e| atproto::drisl::decode(&e.body).unwrap().to_json())
        .collect();
    let last_two = &accounts[accounts.len() - 2..];
    assert_eq!(
        (last_two[0]["active"].clone(), last_two[0]["status"].clone()),
        (json!(false), json!("deactivated"))
    );
    assert_eq!(last_two[1]["active"], true);

    for method in ["deactivateGroup", "activateGroup", "deleteGroup"] {
        let lxm = format!("{NS}.{method}");
        let said = post(&h, &lxm, &json!({}), &[auth(&h, &alice, &lxm)]).await;
        assert_eq!(error(&said), (400, "InvalidRequest"));
        let said = call(&h, &alice, method, "did:plc:nobody").await;
        assert_eq!(error(&said), (400, "GroupNotFound"));
    }
}

#[tokio::test]
async fn a_group_deleted_is_gone() {
    // On http, so that it calls syncers' endpoints here.
    let h = start(&[("PUBLIC_URL", "http://host.test")], false).await;
    let (did, [alice, _, carol]) = group(&h).await;
    let picture = h.host.upload_blob(&did, b"face", "image/png").unwrap();
    h.host
        .space_write(
            &did,
            &meta_space(&did),
            &[Write::Update {
                collection: format!("{NS}.profile"),
                rkey: "self".into(),
                value: json!({"$type": format!("{NS}.profile"), "displayName": "Riders", "joinPolicy": "open", "avatar": picture}),
            }],
        )
        .await
        .unwrap();
    // The services syncing its members space are told it is gone, lapsed
    // or not.
    let (url, syncer) = endpoint().await;
    let members = members_space(&did);
    let to = |token: &str| Registration {
        endpoint: url.clone(),
        token: token.into(),
    };
    let store = &h.host.store;
    store
        .register(&members, &to("live"), now() as i64 + 60)
        .unwrap();
    store
        .register(&members, &to("lapsed"), now() as i64 - 1)
        .unwrap();
    // One that refuses it holds nothing up.
    let (refusing_url, refusing) = endpoint().await;
    refusing.answer(500);
    let refused = Registration {
        endpoint: refusing_url,
        token: "t".into(),
    };
    store
        .register(&members, &refused, now() as i64 + 60)
        .unwrap();
    // A space it ended before is forgotten with it.
    let ended = h
        .host
        .create_space(&did, &format!("{NS}.calendar"), "old", &closed());
    let ended = ended.unwrap();
    h.host.end_space(&did, &ended).unwrap();

    assert_eq!(
        error(&call(&h, &carol, "deleteGroup", &did).await),
        (403, "Forbidden")
    );
    let (status, deleted) = call(&h, &alice, "deleteGroup", &did).await;
    assert_eq!((status, deleted), (200, json!({"tombstoned": true})));

    assert!(h.host.store.account(&did).unwrap().is_none());
    assert!(h.host.store.space(&meta_space(&did)).unwrap().is_none());
    let cid = atproto::Cid::parse(picture["ref"]["$link"].as_str().unwrap()).unwrap();
    assert_eq!(h.host.store.blob(&did, &cid).unwrap(), None);
    // Its DID is ended in the directory.
    let ops = h.plc.ops.lock().unwrap()[&did].clone();
    assert!(matches!(ops.last(), Some(Signed::Tombstone { .. })));
    let (status, _) = get(&h, &format!("{NS}.getGroup"), &format!("group={did}"), &[]).await;
    assert_eq!(status, 400);
    let told = syncer
        .told_of("com.atproto.space.notifySpaceUpdate#spaceDeleted", 2)
        .await;
    let mut tokens: Vec<&str> = told
        .iter()
        .map(|t| {
            assert_eq!(t["space"], members);
            assert_eq!(
                t["update"],
                json!({"$type": "com.atproto.space.notifySpaceUpdate#spaceDeleted"})
            );
            t["token"].as_str().unwrap()
        })
        .collect();
    tokens.sort();
    assert_eq!(tokens, ["lapsed", "live"]);
    refusing.told(1).await;
    // Nothing is kept to try again: the group is gone.
    assert_eq!(store.syncer_notices_due(i64::MAX, 10).unwrap(), vec![]);
    assert!(!store.space_deleted(&ended).unwrap());
}

#[tokio::test]
async fn a_group_switched_off_is_deleted_by_its_stewards_of_then() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    call(&h, &alice, "deactivateGroup", &did).await;
    assert_eq!(
        error(&call(&h, &carol, "deleteGroup", &did).await),
        (403, "Forbidden")
    );
    let (status, deleted) = call(&h, &bob, "deleteGroup", &did).await;
    assert_eq!(status, 200, "{deleted}");
}

#[tokio::test]
async fn a_did_the_directory_will_not_end_keeps_its_account() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    // The directory has forgotten it, so there is no operation to end:
    // the account stays, to be deleted again.
    h.plc.ops.lock().unwrap().remove(&did);
    assert!(h.host.delete_account(&did).await.is_err());
    assert!(h.host.store.account(&did).unwrap().is_some());
}

#[tokio::test]
async fn a_group_the_directory_would_not_end_is_deleted_again() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    turn(&h.plc.refuse, true);
    let said = call(&h, &alice, "deleteGroup", &did).await;
    assert_eq!(said.0, 500);
    turn(&h.plc.refuse, false);
    assert!(h.host.store.account(&did).unwrap().is_some());
    let (status, deleted) = call(&h, &alice, "deleteGroup", &did).await;
    assert_eq!((status, deleted), (200, json!({"tombstoned": true})));
    assert!(h.host.store.account(&did).unwrap().is_none());
}

#[tokio::test]
async fn a_did_ended_already_is_ended() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let last = h.host.plc.last_op(&did).await.unwrap();
    let end = atproto::plc::tombstone(last.cid(), &h.host.secrets.rotation);
    h.host.plc.publish(&did, &end).await.unwrap();
    assert!(h.host.delete_account(&did).await.unwrap());
    assert!(h.host.store.account(&did).unwrap().is_none());
}

#[tokio::test]
async fn a_group_half_made_frees_its_handle() {
    let h = start(&[], false).await;
    let [alice, _, _] = people(&h).await;
    let lxm = format!("{NS}.createGroup");
    // A steward whose DID is no record key: the members space refuses
    // their membership after meta is written.
    let body = json!({"handle": RIDERS, "displayName": "Riders", "stewards": [{"did": "did:web:example.com%20x"}]});
    let said = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    assert!(h.host.store.account(RIDERS).unwrap().is_none());
    start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
}

#[tokio::test]
async fn a_group_half_made_that_cannot_go_fails_as_it_did() {
    let h = start(&[], false).await;
    let [alice, _, _] = people(&h).await;
    let lxm = format!("{NS}.createGroup");
    turn(&h.plc.refuse_ends, true);
    let body = json!({"handle": RIDERS, "displayName": "Riders", "stewards": [{"did": "did:web:example.com%20x"}]});
    let said = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    // Its DID could not be ended, so its account stays.
    assert!(h.host.store.account(RIDERS).unwrap().is_some());
}
