//! Invites: written as the group in the invitee's inbox, through
//! `com.atproto.space.*` with a group token, listed, taken back by
//! deleting them, turned down, and redeemed by asking to join.

mod common;

use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::{members_space, meta_space};
use group_host::group_invites::{inbox_of, millis_of};
use group_host::repo::Write;
use group_host::store::Invite;
use group_host::time::now_millis;
use serde_json::{Value as Json, json};

const NS: &str = "org.atgrp";
const CREATE: &str = "com.atproto.space.createRecord";
const DELETE: &str = "com.atproto.space.deleteRecord";

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = nsid(method);
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn ask(h: &Running, who: &Authority, method: &str, query: &str) -> (u16, Json) {
    let lxm = nsid(method);
    get(h, &lxm, query, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// An invite as an app writes one: the record, with `fields`.
fn invite(fields: Json) -> Json {
    let mut record = json!({"$type": format!("{NS}.invite"), "createdAt": "2026-11-03T00:00:00Z"});
    for (k, v) in fields.as_object().unwrap() {
        record[k] = v.clone();
    }
    json!({"collection": format!("{NS}.invite"), "record": record})
}

/// `who` invites `invitee` to `group`, as the group, the invite holding
/// `fields`: what writing it answered.
async fn send(h: &Running, who: &Authority, group: &str, invitee: &str, fields: Json) -> Said {
    let inbox = inbox_of(invitee);
    inbox_write(h, who, group, &inbox, CREATE, invite(fields)).await
}

/// `who` takes back the invite at `uri`, as the group, by deleting it.
async fn take_back(h: &Running, who: &Authority, group: &str, uri: &str) -> Said {
    let (inbox, rest) = uri.split_once(&format!("/{group}/")).unwrap();
    let rkey = rest.rsplit('/').next().unwrap();
    let body = json!({"collection": format!("{NS}.invite"), "rkey": rkey});
    inbox_write(h, who, group, inbox, DELETE, body).await
}

#[tokio::test]
async fn an_invite_is_written_in_the_invitees_inbox() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "joinPolicy": "invite"}),
    )
    .await;
    let made = send(
        &h,
        &alice,
        &did,
        &bob.did,
        json!({"message": "Ride with us"}),
    )
    .await;
    assert_eq!(made.status, 200, "{}", made.body);
    let uri = made.body["uri"].as_str().unwrap().to_string();
    let inbox = inbox_of(&bob.did);
    assert!(uri.starts_with(&format!("{inbox}/{did}/{NS}.invite/")));
    // In the group's repo in Bob's inbox, and told to his PDS, its authority.
    let rkey = uri.rsplit('/').next().unwrap();
    let record = h
        .host
        .group_record(&did, &inbox, &format!("{NS}.invite"), rkey)
        .unwrap()
        .unwrap();
    // What it says, and no word of who sent it.
    assert_eq!(
        (record.get("by"), record["message"].clone()),
        (None, json!("Ride with us"))
    );
    assert_eq!(bob.told.writes.lock().unwrap()[0].1["space"], inbox);
    // The group's activity says the write was in Bob's inbox, and about him.
    let (_, listed) = ask(&h, &alice, "listGroupTokens", &format!("group={did}")).await;
    let activity = listed["activity"].as_array().unwrap();
    let wrote = activity.iter().find(|a| a.get("write").is_some()).unwrap();
    assert_eq!(wrote["write"]["space"], inbox);
    assert_eq!(wrote["subject"]["did"], bob.did.as_str());
    // Written again, the same one answered, as it is.
    let again = send(&h, &alice, &did, &bob.did, json!({"message": "Hello"})).await;
    assert_eq!(
        (again.body["uri"].clone(), again.body["cid"].clone()),
        (json!(uri), made.body["cid"].clone())
    );

    // Listed for those who invite, newest first, a page at a time; its
    // lapse as given, at an offset from UTC or in it.
    let soon = "2099-01-01T01:00:00.5+01:00";
    send(&h, &alice, &did, &carol.did, json!({"expiresAt": soon})).await;
    let (_, page) = ask(&h, &alice, "listInvites", &format!("group={did}&limit=1")).await;
    assert_eq!(
        (
            page["invites"][0]["invitee"].clone(),
            page["invites"][0]["expiresAt"].clone(),
            page["invites"][0]["actor"]["did"].clone()
        ),
        (
            json!(carol.did),
            json!("2099-01-01T00:00:00.500Z"),
            json!(carol.did)
        )
    );
    let cursor = page["cursor"].as_str().unwrap();
    let (_, rest) = ask(
        &h,
        &alice,
        "listInvites",
        &format!("group={did}&cursor={cursor}"),
    )
    .await;
    assert_eq!(
        (
            rest["invites"][0]["uri"].clone(),
            rest["invites"][0]["message"].clone()
        ),
        (json!(uri), json!("Ride with us"))
    );

    // Bob sees his, from the group here.
    let (_, mine) = ask(&h, &bob, "getActorInvites", "").await;
    assert_eq!(mine["invites"][0]["group"]["displayName"], "Riders");
    assert_eq!(mine["invites"][0].get("by"), None);
    let (_, filtered) = ask(&h, &bob, "getActorInvites", "group=did:plc:other").await;
    assert_eq!(filtered["invites"], json!([]));

    // A request of theirs from before, gone once they are in.
    h.host.store.ask(&did, &bob.did, None, 0).unwrap();
    // Redeemed by asking to join, naming it: admitted, though the group
    // invites only, and the invite taken back. One named that is not the
    // invite waiting for them is refused.
    for named in [
        format!("{inbox}/{did}/{NS}.invite/3zzzzzzzzzzzz"),
        uri.replace(&bob.did, &carol.did),
    ] {
        let said = call(
            &h,
            &bob,
            "requestJoin",
            json!({"group": did, "invite": named}),
        )
        .await;
        assert_eq!(error(&said), (400, "InvalidInvite"));
    }
    let said = call(
        &h,
        &bob,
        "requestJoin",
        json!({"group": did, "invite": uri}),
    )
    .await;
    assert_eq!((said.0, &said.1["status"]), (200, &json!("admitted")));
    assert!(
        said.1["membership"]["uri"]
            .as_str()
            .unwrap()
            .ends_with(&bob.did)
    );
    assert_eq!(h.host.roles(&did, &bob.did).unwrap(), ["member"]);
    assert_eq!(
        h.host
            .group_record(&did, &inbox, &format!("{NS}.invite"), rkey)
            .unwrap(),
        None
    );
    assert_eq!(
        ask(&h, &bob, "getActorInvites", "").await.1["invites"],
        json!([])
    );

    for (who, invitee, fields, refused) in [
        (&alice, bob.did.as_str(), json!({}), (400, "InvalidRequest")),
        (&alice, "bob", json!({}), (400, "InvalidRequest")),
        (
            &alice,
            carol.did.as_str(),
            json!({"expiresAt": "2027-02-30T00:00:00Z"}),
            (400, "InvalidRequest"),
        ),
        // One lapsed already would admit nobody.
        (
            &alice,
            carol.did.as_str(),
            json!({"expiresAt": "2020-01-01T00:00:00Z"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            carol.did.as_str(),
            json!({"message": "ü".repeat(2001)}),
            (400, "InvalidRequest"),
        ),
        // Bob may not invite: no token for it.
        (&bob, carol.did.as_str(), json!({}), (403, "Forbidden")),
    ] {
        let said = send(&h, who, &did, invitee, fields.clone()).await;
        assert_eq!(said.error(), refused, "{invitee} {fields}");
    }
    assert_eq!(
        error(&ask(&h, &bob, "listInvites", &format!("group={did}")).await),
        (403, "Forbidden")
    );
    assert_eq!(
        error(&ask(&h, &alice, "listInvites", &format!("group={did}&cursor=x")).await),
        (400, "InvalidRequest")
    );
}

#[tokio::test]
async fn a_token_writes_invites_only_as_far_as_it_and_its_member_may() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let app = App::new("console");
    let inbox = inbox_of(&bob.did);
    let given = group_token(&h, &app, &alice, &did, &inbox).await;
    assert_eq!(given.status, 200, "{}", given.body);
    assert_eq!(
        given.body["scope"],
        format!(
            "atproto space:{NS}.inbox?authority={}&skey=self&collection={NS}.invite&action=create&action=update&action=delete",
            bob.did
        )
    );
    assert_eq!(
        (
            given.body["collections"].clone(),
            given.body["repoCollections"].clone()
        ),
        (json!([{"collection": format!("{NS}.invite")}]), json!([]))
    );
    let token = given.body["accessJwt"].as_str().unwrap().to_string();
    let write = async |nsid: &str, mut body: Json| {
        body["repo"] = json!(did);
        if body.get("space").is_none() {
            body["space"] = json!(inbox);
        }
        app.call(&h, nsid, &token, Some(&body)).await
    };

    // Its one inbox, and invites alone there, one at a time.
    let elsewhere = json!({"space": inbox_of(&carol.did)});
    let mut body = invite(json!({}));
    body["space"] = elsewhere["space"].clone();
    assert_eq!(write(CREATE, body).await.error(), (403, "ScopeMissing"));
    let mut rule = invite(json!({}));
    rule["collection"] = json!(format!("{NS}.rule"));
    assert_eq!(write(CREATE, rule).await.error(), (400, "InvalidRequest"));
    let record = invite(json!({}))["record"].clone();
    let one = json!({"$type": "com.atproto.space.applyWrites#create", "collection": format!("{NS}.invite"), "value": record});
    let two = json!({"writes": [one, one]});
    let apply = "com.atproto.space.applyWrites";
    assert_eq!(write(apply, two).await.error(), (400, "InvalidRequest"));
    let said = write(apply, json!({"writes": [one]})).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let rkey = said.body["results"][0]["uri"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();

    // Put at its key, it replaces the one waiting.
    let put = json!({
        "collection": format!("{NS}.invite"),
        "rkey": rkey,
        "record": invite(json!({"message": "Come along"}))["record"],
    });
    let said = write("com.atproto.space.putRecord", put).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let listed = ask(&h, &alice, "listInvites", &format!("group={did}"))
        .await
        .1;
    assert_eq!(listed["invites"].as_array().unwrap().len(), 1);
    assert_eq!(listed["invites"][0]["message"], "Come along");

    // An inbox is at self.
    let odd = format!("at:space/{}/{NS}.inbox/other", bob.did);
    let said = inbox_write(&h, &alice, &did, &odd, CREATE, invite(json!({}))).await;
    assert_eq!(said.error(), (400, "InvalidRequest"));

    // Its member no longer able to invite, it writes no invite.
    let founder = json!({"$type": format!("{NS}.membership"), "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"});
    let rkey_of = |d: &str| group_host::group::did_rkey(d);
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[Write::Update {
                collection: format!("{NS}.membership"),
                rkey: rkey_of(&alice.did),
                value: founder,
            }],
        )
        .await
        .unwrap();
    let said = write(CREATE, invite(json!({}))).await;
    assert_eq!(said.error(), (403, "Forbidden"));
}

#[tokio::test]
async fn invites_are_taken_back_turned_down_or_lapse() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "joinPolicy": "invite"}),
    )
    .await;
    // A face, and no name, as a group moved here from a host that kept
    // none may have: the invitee's list shows the face alone.
    let picture = h.host.upload_blob(&did, b"face", "image/png").unwrap();
    h.host
        .space_write(
            &did,
            &meta_space(&did),
            &[Write::Update {
                collection: format!("{NS}.about"),
                rkey: "self".into(),
                value: json!({"$type": format!("{NS}.about"), "avatar": picture}),
            }],
        )
        .await
        .unwrap();
    let made = send(&h, &alice, &did, &bob.did, json!({})).await;
    let uri = made.body["uri"].as_str().unwrap().to_string();
    let shown = ask(&h, &bob, "getActorInvites", "").await.1;
    let group = &shown["invites"][0]["group"];
    assert_eq!(group["avatar"], picture["ref"]["$link"]);
    assert_eq!(group["handle"], RIDERS);
    assert_eq!(group.get("displayName"), None);
    // Taken back by deleting it; then there is none to take back.
    let said = take_back(&h, &alice, &did, &uri).await;
    assert_eq!((said.status, said.body), (200, json!({})));
    assert_eq!(
        take_back(&h, &alice, &did, &uri).await.error(),
        (400, "InviteNotFound")
    );
    assert_eq!(
        error(&call(&h, &bob, "requestJoin", json!({"group": did})).await),
        (400, "JoinClosed")
    );

    // Turned down by its invitee.
    send(&h, &alice, &did, &bob.did, json!({})).await;
    assert_eq!(
        call(&h, &bob, "declineInvite", json!({"group": did})).await,
        (200, json!({}))
    );
    assert_eq!(
        error(&call(&h, &bob, "declineInvite", json!({"group": did})).await),
        (400, "InviteNotFound")
    );
    assert_eq!(
        error(&call(&h, &bob, "declineInvite", json!({})).await),
        (400, "InvalidRequest")
    );

    // Lapsed, it is no invite, and is taken back once found so. A past
    // expiry is refused, so these lapse three seconds after they are made:
    // time enough for both to be sent, on a runner under load.
    let lapse = group_host::time::iso(now_millis() + 3000);
    let lapsed = json!({ "expiresAt": lapse });
    let made = send(&h, &alice, &did, &carol.did, lapsed.clone()).await;
    assert_eq!(made.status, 200, "{}", made.body);
    let bobs = send(&h, &alice, &did, &bob.did, lapsed).await;
    assert_eq!(bobs.status, 200, "{}", bobs.body);
    tokio::time::sleep(std::time::Duration::from_millis(3100)).await;
    assert_eq!(
        ask(&h, &alice, "listInvites", &format!("group={did}"))
            .await
            .1["invites"],
        json!([])
    );
    assert_eq!(
        error(&call(&h, &carol, "requestJoin", json!({"group": did})).await),
        (400, "JoinClosed")
    );
    let gone = |made: &Json, invitee: &str| {
        let rkey = made["uri"].as_str().unwrap().rsplit('/').next().unwrap();
        let record = h
            .host
            .group_record(&did, &inbox_of(invitee), &format!("{NS}.invite"), rkey)
            .unwrap();
        record.is_none() && h.host.store.invited(&did, invitee).unwrap().is_none()
    };
    assert!(gone(&made.body, &carol.did));
    // Or by the host's sweep, asked of no one.
    let made = bobs.body;
    assert!(!gone(&made, &bob.did));
    assert_eq!(h.host.sweep_invites(now_millis()).await.unwrap(), 1);
    assert!(gone(&made, &bob.did));
    assert_eq!(h.host.sweep_invites(now_millis()).await.unwrap(), 0);
    // Invited again, a fresh invite, which admits; its message as long as
    // the lexicon lets it be.
    let words = json!({"message": "ü".repeat(2000)});
    let again = send(&h, &alice, &did, &carol.did, words).await;
    let listed = ask(&h, &alice, "listInvites", &format!("group={did}"))
        .await
        .1;
    assert_eq!(listed["invites"][0]["uri"], again.body["uri"]);
    assert_eq!(
        call(&h, &carol, "requestJoin", json!({"group": did}))
            .await
            .1["status"],
        "admitted"
    );

    // A group switched off shows no invites.
    let dave = authority(&h, false, true).await;
    send(&h, &alice, &did, &dave.did, json!({})).await;
    h.host.deactivate_account(&did, &alice.did, &[]).unwrap();
    assert_eq!(
        ask(&h, &dave, "getActorInvites", "").await.1["invites"],
        json!([])
    );
    // Its lapsed invites wait for it to be on again.
    let erin = "did:plc:erin";
    h.host
        .store
        .invite(&Invite {
            seq: 0,
            group: did.clone(),
            invitee: erin.into(),
            by: alice.did.clone(),
            message: None,
            rkey: "3lapsed".into(),
            created_at: 0,
            expires_at: Some(1),
        })
        .unwrap();
    assert_eq!(h.host.sweep_invites(now_millis()).await.unwrap(), 0);
    assert!(h.host.store.invited(&did, erin).unwrap().is_some());
}

#[test]
fn reads_a_datetime() {
    assert_eq!(millis_of("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(millis_of("2000-03-01T00:00:00.25Z"), Some(951_868_800_250));
    assert_eq!(millis_of("2026-11-03T12:30:15Z"), Some(1_793_709_015_000));
    assert_eq!(millis_of("2024-02-29T00:00:00Z"), Some(1_709_164_800_000));
    // At an offset from UTC, the same moment.
    assert_eq!(
        millis_of("2026-11-03T13:30:15+01:00"),
        Some(1_793_709_015_000)
    );
    assert_eq!(
        millis_of("2026-11-03T12:00:15-00:30"),
        Some(1_793_709_015_000)
    );
    for bad in [
        "x",
        "2026-11-03T00:00:00-00:00",
        "2026-11-03T00:00:00*01:00",
        "2026-11-03T00:00:00+0100",
        "2026-11-03T00:00:00+24:00",
        "2026-11-03T00:00:00+01:60",
        "+026-11-03T00:00:00Z",
        "2026-11-03T00:00:00.+5Z",
        "2026-11-31T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2026-1-03T00:00:00Z",
        "2026-11-03T00:00:00.x",
        "2026-13-01T00:00:00Z",
        "2026-11-03T00:00:00",
        "2026-11-03",
        "2026-11-03T00:00Z",
        "x-11-03T00:00:00Z",
        "2026-11-03-1T00:00:00Z",
        "2026-11-03T00:00:00:00Z",
        "2026-11-03T25:00:00Z",
    ] {
        assert_eq!(millis_of(bad), None, "{bad}");
    }
}
