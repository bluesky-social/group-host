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
                value: json!({"$type": format!("{NS}.membership"), "member": bob.did, "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}),
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

/// Grants to members to read a calendar.
fn readers() -> Json {
    json!([{"role": "member", "scopes": [format!("space:{CALENDAR}?action=read")]}])
}

#[tokio::test]
async fn an_admin_makes_a_space() {
    let h = start(&[], false).await;
    let (did, [alice, bob, carol]) = group(&h).await;
    let grants = json!([
        {"role": "member", "scopes": [format!("space:{CALENDAR}?action=read")]},
        {"role": "admin", "scopes": [format!("space:{CALENDAR}?collection=fyi.opensocial.event")]},
    ]);
    let (status, made) = call(
        &h,
        &alice,
        "createSpace",
        json!({
            "group": did,
            "type": CALENDAR,
            "skey": "rides",
            "name": "Rides",
            "access": {"grants": grants, "writableBy": [{"role": "admin"}]},
        }),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let uri = format!("at://{did}/space/{CALENDAR}/rides");
    assert_eq!(made["space"], uri);
    // It answers what it wrote: the access record and the index entry.
    let a = access(&h, &did, &uri);
    assert_eq!(
        made["access"]["uri"],
        format!("{uri}/{did}/{NS}.access/self")
    );
    let kept = h
        .host
        .store
        .space_record(&did, &uri, &format!("{NS}.access"), "self")
        .unwrap()
        .unwrap();
    assert_eq!(made["access"]["cid"], kept.cid.to_string());
    let index = made["index"]["uri"].as_str().unwrap();
    assert!(index.starts_with(&format!("{}/{did}/{NS}.space/", members_space(&did))));
    assert_eq!(a["grants"], grants);
    assert_eq!(a.get("public"), None);
    assert_eq!(a["writableBy"], json!([{"role": "admin"}]));
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

    // With no key, one of its own; with no writers named, any member
    // granted read; public, read by anyone signed in.
    let (_, other) = call(
        &h,
        &alice,
        "createSpace",
        json!({"group": did, "type": CALENDAR, "access": {"public": true, "grants": readers(), "writableBy": []}}),
    )
    .await;
    let other = other["space"].as_str().unwrap();
    assert_ne!(other, uri);
    assert_eq!(access(&h, &did, other).get("writableBy"), None);
    assert_eq!(access(&h, &did, other)["public"], true);
    assert_eq!(entry(&h, &did, other).unwrap().get("name"), None);
    let hosted = h.host.store.space(other).unwrap().unwrap();
    assert!(
        h.host
            .may(other, &hosted, Some(&bob.did), Access::Write)
            .unwrap()
    );
    assert!(
        h.host
            .may(other, &hosted, Some(&carol.did), Access::Read)
            .unwrap()
    );
    assert!(!h.host.may(other, &hosted, None, Access::Read).unwrap());

    // Left out of the index, it has no entry, and says so.
    let (status, hidden) = call(
        &h,
        &alice,
        "createSpace",
        json!({"group": did, "type": CALENDAR, "skey": "hidden", "index": false, "access": {"grants": readers()}}),
    )
    .await;
    assert_eq!(status, 200, "{hidden}");
    assert_eq!(hidden.get("index"), None);
    assert_eq!(entry(&h, &did, hidden["space"].as_str().unwrap()), None);

    let read =
        |role: &str| json!([{"role": role, "scopes": [format!("space:{CALENDAR}?action=read")]}]);
    for (who, body, refused) in [
        (
            &bob,
            json!({"group": did, "type": CALENDAR, "access": {"grants": readers()}}),
            (403, "Forbidden"),
        ),
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "skey": "rides", "access": {"grants": readers()}}),
            (400, "SpaceAlreadyExists"),
        ),
        (
            &alice,
            json!({"group": did, "type": "calendar", "access": {"grants": readers()}}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "type": CALENDAR}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "access": {"grants": ["member"]}}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"type": CALENDAR, "access": {"grants": readers()}}),
            (400, "InvalidRequest"),
        ),
        // The meta and members spaces come with the group.
        (
            &alice,
            json!({"group": did, "type": format!("{NS}.meta"), "skey": "other", "access": {"grants": readers()}}),
            (400, "ReservedSpace"),
        ),
        (
            &alice,
            json!({"group": did, "type": format!("{NS}.members"), "access": {"grants": readers()}}),
            (400, "ReservedSpace"),
        ),
        // A role the group has not, or a grant that is no space scope.
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "access": {"grants": read("guest")}}),
            (400, "UnknownRole"),
        ),
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "access": {"grants": [{"role": "member", "scopes": ["repo:fyi.opensocial.event"]}]}}),
            (400, "InvalidScope"),
        ),
        // One grant a role.
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "access": {"grants": [read("member")[0], read("member")[0]]}}),
            (400, "InvalidRequest"),
        ),
        // Nobody to read it, as no change takes either: no grants, or
        // read granted in another space.
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "access": {"grants": []}}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "access": {"grants": [{"role": "member", "scopes": ["space:fyi.opensocial.forum?action=read"]}]}}),
            (400, "InvalidRequest"),
        ),
        // A name longer than the lexicon has it.
        (
            &alice,
            json!({"group": did, "type": CALENDAR, "access": {"grants": readers()}, "name": "x".repeat(65)}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = call(&h, who, "createSpace", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }

    // An access record that is no record (a float, which its lexicon lets
    // through and DRISL does not) leaves no space behind, and its
    // key free to be made again.
    let wrong = json!({"group": did, "type": CALENDAR, "skey": "laps", "access": {"grants": [{"role": "member", "scopes": [format!("space:{CALENDAR}?action=read")], "weight": 1.5}]}});
    let said = call(&h, &alice, "createSpace", wrong).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    let laps = format!("at://{did}/space/{CALENDAR}/laps");
    assert!(h.host.store.space(&laps).unwrap().is_none());
    assert!(h.host.store.space_head(&did, &laps).unwrap().is_none());
    let right =
        json!({"group": did, "type": CALENDAR, "skey": "laps", "access": {"grants": readers()}});
    let said = call(&h, &alice, "createSpace", right).await;
    assert_eq!(said.0, 200);
    assert_eq!(said.1["space"], laps);
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
        json!({"group": did, "type": CALENDAR, "skey": "rides", "access": {"grants": readers(), "writableBy": [{"role": "admin"}]}}),
    )
    .await;
    let uri = made["space"].as_str().unwrap().to_string();
    let grants = json!([{"role": "member", "scopes": [format!("space:{CALENDAR}?collection=*")]}]);
    let (status, _) = call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": uri, "name": "Rides", "access": {"public": true, "grants": grants}}),
    )
    .await;
    assert_eq!(status, 200);
    // Replaced whole: who writes is every member granted read again.
    let a = access(&h, &did, &uri);
    assert_eq!(a["public"], true);
    assert_eq!(a["grants"], grants);
    assert_eq!(a.get("writableBy"), None);
    assert_eq!(entry(&h, &did, &uri).unwrap()["name"], "Rides");
    call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": uri, "access": {"grants": readers(), "writableBy": [{"role": "member"}]}}),
    )
    .await;
    let a = access(&h, &did, &uri);
    assert_eq!(a["writableBy"], json!([{"role": "member"}]));
    assert_eq!(a.get("public"), None);
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
    assert_eq!(access(&h, &did, &uri), a);

    // Meta's readers are the group's visibility: its declaration follows.
    let declared = |h: &Running| {
        h.host
            .store
            .record(&did, &format!("{NS}.declaration"), "self")
            .unwrap()
            .is_some()
    };
    let meta = meta_space(&did);
    let members_read =
        json!([{"role": "member", "scopes": [format!("space:{NS}.meta?action=read")]}]);
    assert!(declared(&h));
    for (public, listed) in [(false, false), (false, false), (true, true), (true, true)] {
        let said = call(
            &h,
            &alice,
            "updateSpace",
            json!({"group": did, "space": meta, "access": {"public": public, "grants": members_read}}),
        )
        .await;
        assert_eq!(said.0, 200, "{}", said.1);
        assert_eq!(declared(&h), listed);
    }

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
        let mut value = json!({"$type": format!("{NS}.access"), "grants": []});
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
    let admins_read =
        json!([{"role": "admin", "scopes": [format!("space:{NS}.members?action=read")]}]);
    let members_space_read =
        json!([{"role": "member", "scopes": [format!("space:{NS}.members?action=read")]}]);
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
            json!({"group": did, "space": unlisted, "name": "Named", "access": {"public": true, "grants": []}}),
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
            json!({"group": did, "space": members, "access": {"grants": admins_read}}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "space": members, "access": {"grants": members_space_read, "writableBy": [{"role": "admin"}]}}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "space": uri, "access": {"grants": readers(), "writableBy": "admin"}}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "space": uri, "access": {"grants": [{"role": "guest", "scopes": []}]}}),
            (400, "UnknownRole"),
        ),
        // Nobody to read it is no change it takes.
        (
            &alice,
            json!({"group": did, "space": uri, "access": {"grants": []}}),
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
    assert_eq!(access(&h, &did, &unlisted).get("public"), None);
    // The members space may change, readable by members still.
    let both = json!([
        {"role": "member", "scopes": [format!("space:{NS}.members?action=read")]},
        {"role": "admin", "scopes": [format!("space:{NS}.members?action=read")]},
    ]);
    let said = call(
        &h,
        &alice,
        "updateSpace",
        json!({"group": did, "space": members, "access": {"grants": both}}),
    )
    .await;
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
        json!({"group": did, "type": CALENDAR, "skey": "rides", "access": {"grants": readers()}}),
    )
    .await;
    let uri = made["space"].as_str().unwrap().to_string();
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
        assert_eq!(error(&said), (400, "ReservedSpace"));
    }
    // One left out of the index ends as well; one never made is not found.
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
                value: json!({"$type": format!("{NS}.access"), "grants": []}),
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
    assert_eq!(said, (200, json!({})));
    assert!(h.host.store.space(&unlisted).unwrap().is_none());
    let said = call(
        &h,
        &alice,
        "deleteSpace",
        json!({"group": did, "space": unlisted}),
    )
    .await;
    assert_eq!(error(&said), (400, "SpaceNotFound"));
}
