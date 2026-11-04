//! The spaces a group's members make beside its two own: made, changed
//! and ended by those whose roles allow it.

mod common;

use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use group_host::authority::Access;
use group_host::group::{members_space, meta_space};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const NS: &str = "fyi.opensocial";
const CALENDAR: &str = "fyi.opensocial.calendar";

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
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
                value: json!({"$type": format!("{NS}.membership"), "member": bob.did, "roles": [{"role": "member"}], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    (did, [alice, bob, carol])
}

fn access(h: &Running, did: &str, space: &str) -> Json {
    h.host
        .group_record(did, space, &format!("{NS}.access"), "self")
        .unwrap()
        .unwrap()
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

#[tokio::test]
async fn an_admin_makes_a_space() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let scopes = json!([{"role": "admin", "scopes": [{"collection": "fyi.opensocial.event"}]}]);
    let (status, made) = call(
        &h,
        &alice,
        "createSpace",
        json!({
            "group": did,
            "spaceType": CALENDAR,
            "skey": "rides",
            "name": "Rides",
            "readableBy": [{"role": "member"}],
            "writableBy": [{"role": "admin"}],
            "credentialScopes": scopes,
        }),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let uri = format!("at://{did}/space/{CALENDAR}/rides");
    assert_eq!(made["uri"], uri);
    let a = access(&h, &did, &uri);
    assert_eq!(a["readableBy"], json!([{"role": "member"}]));
    assert_eq!(a["writableBy"], json!([{"role": "admin"}]));
    assert_eq!(a["credentialScopes"], scopes);
    assert_eq!(entry(&h, &did, &uri).unwrap()["name"], "Rides");
    // The group is its authority, and its access record decides.
    let hosted = h.host.store.space(&uri).unwrap().unwrap();
    assert!(
        h.host
            .may(&uri, &hosted, Some(&bob.did), Access::Read)
            .unwrap()
    );
    assert!(
        !h.host
            .may(&uri, &hosted, Some(&bob.did), Access::Write)
            .unwrap()
    );
    assert!(
        !h.host
            .may(&uri, &hosted, Some(&carol.did), Access::Read)
            .unwrap()
    );

    // With no key, one of its own; with no writers named, any member.
    let (_, other) = call(
        &h,
        &alice,
        "createSpace",
        json!({"group": did, "spaceType": CALENDAR, "readableBy": [{"role": "member"}], "writableBy": []}),
    )
    .await;
    let other = other["uri"].as_str().unwrap();
    assert_ne!(other, uri);
    assert_eq!(access(&h, &did, other).get("writableBy"), None);
    assert_eq!(entry(&h, &did, other).unwrap().get("name"), None);

    for (who, body, refused) in [
        (
            &bob,
            json!({"group": did, "spaceType": CALENDAR, "readableBy": []}),
            (403, "Forbidden"),
        ),
        (
            &alice,
            json!({"group": did, "spaceType": CALENDAR, "skey": "rides", "readableBy": [{"role": "member"}]}),
            (400, "SpaceAlreadyExists"),
        ),
        (
            &alice,
            json!({"group": did, "spaceType": "calendar", "readableBy": []}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "spaceType": CALENDAR}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "spaceType": CALENDAR, "readableBy": ["member"]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"spaceType": CALENDAR, "readableBy": []}),
            (400, "InvalidRequest"),
        ),
        // Nobody to read it, as no change takes either.
        (
            &alice,
            json!({"group": did, "spaceType": CALENDAR, "readableBy": []}),
            (400, "InvalidRequest"),
        ),
        // A name longer than the lexicon has it.
        (
            &alice,
            json!({"group": did, "spaceType": CALENDAR, "readableBy": [{"role": "member"}], "name": "x".repeat(65)}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = call(&h, who, "createSpace", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }

    // An access record that is no record leaves no space behind, and its
    // key free to be made again.
    let wrong = json!({"group": did, "spaceType": CALENDAR, "skey": "laps", "readableBy": [{"role": "member"}], "credentialScopes": [{"role": "admin", "scopes": 1.5}]});
    let said = call(&h, &alice, "createSpace", wrong).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    let laps = format!("at://{did}/space/{CALENDAR}/laps");
    assert!(h.host.store.space(&laps).unwrap().is_none());
    assert!(h.host.store.space_head(&did, &laps).unwrap().is_none());
    let right = json!({"group": did, "spaceType": CALENDAR, "skey": "laps", "readableBy": [{"role": "member"}]});
    let said = call(&h, &alice, "createSpace", right).await;
    assert_eq!(said, (200, json!({"uri": laps})));
    assert!(entry(&h, &did, &laps).is_some());
}

#[tokio::test]
async fn an_admin_changes_a_space() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let (_, made) = call(
        &h,
        &alice,
        "createSpace",
        json!({"group": did, "spaceType": CALENDAR, "skey": "rides", "readableBy": [{"role": "member"}], "writableBy": [{"role": "admin"}]}),
    )
    .await;
    let uri = made["uri"].as_str().unwrap().to_string();
    let made_at = access(&h, &did, &uri)["createdAt"].clone();
    let scopes = json!([{"role": "member", "scopes": [{"collection": "*"}]}]);
    let (status, _) = call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": uri, "name": "Rides", "readableBy": [{"role": "public"}], "credentialScopes": scopes}),
    )
    .await;
    assert_eq!(status, 200);
    let a = access(&h, &did, &uri);
    assert_eq!(a["readableBy"], json!([{"role": "public"}]));
    assert_eq!(a["writableBy"], json!([{"role": "admin"}]));
    assert_eq!(a["credentialScopes"], scopes);
    assert_eq!(a["createdAt"], made_at);
    assert_eq!(entry(&h, &did, &uri).unwrap()["name"], "Rides");
    // Who writes, set and put back to any member.
    call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": uri, "writableBy": [{"role": "member"}]}),
    )
    .await;
    assert_eq!(
        access(&h, &did, &uri)["writableBy"],
        json!([{"role": "member"}])
    );
    call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": uri, "writableBy": []}),
    )
    .await;
    assert_eq!(access(&h, &did, &uri).get("writableBy"), None);
    // Asked to change nothing, nothing changes.
    assert_eq!(
        call(
            &h,
            &alice,
            "updateSpace",
            json!({"group": did, "space": uri})
        )
        .await
        .0,
        200
    );

    // Meta's readers are the group's visibility: its declaration follows.
    let declared = |h: &Running| {
        h.host
            .store
            .record(&did, &format!("{NS}.declaration"), "self")
            .unwrap()
            .is_some()
    };
    let meta = meta_space(&did);
    assert!(declared(&h));
    call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": meta, "readableBy": [{"role": "member"}]}),
    )
    .await;
    assert!(!declared(&h));
    call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": meta, "readableBy": [{"role": "member"}]}),
    )
    .await;
    call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": meta, "readableBy": [{"role": "public"}]}),
    )
    .await;
    assert!(declared(&h));
    call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": meta, "readableBy": [{"role": "public"}]}),
    )
    .await;
    assert!(declared(&h));

    // A space for chosen people, and one in no index.
    let chosen = h
        .host
        .create_space(&did, CALENDAR, "chosen", false)
        .unwrap();
    let unlisted = h
        .host
        .create_space(&did, CALENDAR, "unlisted", false)
        .unwrap();
    for (space, people) in [
        (&chosen, Some(json!([{"did": bob.did}]))),
        (&unlisted, None),
    ] {
        let mut value = json!({"$type": format!("{NS}.access"), "readableBy": [], "createdAt": "2026-11-03T00:00:00Z"});
        if let Some(people) = people {
            value["people"] = people;
        }
        h.host
            .space_write(
                &did,
                space,
                &[Write::Create {
                    collection: format!("{NS}.access"),
                    rkey: "self".into(),
                    value,
                }],
            )
            .await
            .unwrap();
    }
    let members = members_space(&did);
    let elsewhere = format!("at://did:plc:elsewhere/space/{CALENDAR}/rides");
    for (who, body, refused) in [
        (
            &bob,
            json!({"group": did, "space": uri, "name": "Mine"}),
            (403, "Forbidden"),
        ),
        (
            &alice,
            json!({"group": did, "space": chosen, "name": "Ours"}),
            (403, "Forbidden"),
        ),
        (
            &alice,
            json!({"group": did, "space": unlisted, "name": "Named", "readableBy": [{"role": "public"}]}),
            (400, "SpaceNotFound"),
        ),
        (
            &alice,
            json!({"group": did, "space": format!("at://{did}/space/{CALENDAR}/none")}),
            (400, "SpaceNotFound"),
        ),
        (
            &alice,
            json!({"group": did, "space": elsewhere}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "space": "nope"}),
            (400, "InvalidRequest"),
        ),
        (&alice, json!({"group": did}), (400, "InvalidRequest")),
        (
            &alice,
            json!({"group": did, "space": members, "readableBy": [{"role": "admin"}]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "space": members, "writableBy": [{"role": "admin"}]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "space": uri, "writableBy": "admin"}),
            (400, "InvalidRequest"),
        ),
        // Nobody to read it is no change it takes.
        (
            &alice,
            json!({"group": did, "space": uri, "readableBy": []}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "space": uri, "name": "x".repeat(65)}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = call(&h, who, "updateSpace", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }
    // A change refused for want of the space's entry changes nothing.
    assert_eq!(access(&h, &did, &unlisted)["readableBy"], json!([]));
    // The members space may change, readable by members still.
    let said = call(&h, &alice, "updateSpace", json!({"group": did, "space": members, "readableBy": [{"role": "member"}, {"role": "admin"}]})).await;
    assert_eq!(said.0, 200);
}

#[tokio::test]
async fn an_admin_ends_a_space() {
    let h = start(&[], false).await;
    let (did, [alice, bob, _]) = group(&h).await;
    let (_, made) = call(
        &h,
        &alice,
        "createSpace",
        json!({"group": did, "spaceType": CALENDAR, "skey": "rides", "readableBy": [{"role": "member"}]}),
    )
    .await;
    let uri = made["uri"].as_str().unwrap().to_string();
    let picture = h.host.upload_blob(&did, b"map", "image/png").unwrap();
    h.host
        .space_write(
            &did,
            &uri,
            &[Write::Create {
                collection: "fyi.opensocial.event".into(),
                rkey: "first".into(),
                value: json!({"$type": "fyi.opensocial.event", "name": "First ride", "map": picture}),
            }],
        )
        .await
        .unwrap();
    let syncer = authority(&h, false, true).await;
    h.host
        .store
        .register(&uri, &syncer.did, &syncer.pds, now() as i64 + 60)
        .unwrap();

    assert_eq!(
        error(&call(&h, &bob, "deleteSpace", json!({"group": did, "space": uri})).await),
        (403, "Forbidden")
    );
    assert_eq!(
        call(
            &h,
            &alice,
            "deleteSpace",
            json!({"group": did, "space": uri})
        )
        .await,
        (200, json!({}))
    );
    assert_eq!(entry(&h, &did, &uri), None);
    assert!(h.host.store.space(&uri).unwrap().is_none());
    assert!(h.host.store.space_head(&did, &uri).unwrap().is_none());
    let cid = atproto::Cid::parse(picture["ref"]["$link"].as_str().unwrap()).unwrap();
    assert_eq!(h.host.store.blob(&did, &cid).unwrap(), None);
    for _ in 0..100 {
        if !syncer.told.writes.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        syncer.told.writes.lock().unwrap()[0].1,
        json!({"space": uri})
    );

    for space in [meta_space(&did), members_space(&did)] {
        let said = call(
            &h,
            &alice,
            "deleteSpace",
            json!({"group": did, "space": space}),
        )
        .await;
        assert_eq!(error(&said), (400, "WellKnownSpace"));
    }
    let unlisted = h
        .host
        .create_space(&did, CALENDAR, "unlisted", false)
        .unwrap();
    h.host
        .space_write(
            &did,
            &unlisted,
            &[Write::Create {
                collection: format!("{NS}.access"),
                rkey: "self".into(),
                value: json!({"$type": format!("{NS}.access"), "readableBy": [], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    let said = call(
        &h,
        &alice,
        "deleteSpace",
        json!({"group": did, "space": unlisted}),
    )
    .await;
    assert_eq!(error(&said), (400, "SpaceNotFound"));
}
