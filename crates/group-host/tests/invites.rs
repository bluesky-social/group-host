//! Invites: written in the invitee's inbox, listed, taken back, turned
//! down, and redeemed by asking to join; and who may read and write an
//! inbox, as its authority asks.

mod common;

use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::meta_space;
use group_host::group_invites::{inbox_of, millis_of};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const NS: &str = "fyi.opensocial";

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn ask(h: &Running, who: &Authority, method: &str, query: &str) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    get(h, &lxm, query, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
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
    let (status, made) = call(
        &h,
        &alice,
        "createInvite",
        json!({"group": did, "invitee": bob.did, "message": "Ride with us"}),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let uri = made["uri"].as_str().unwrap().to_string();
    let inbox = inbox_of(&bob.did);
    assert!(uri.starts_with(&format!("{inbox}/{did}/{NS}.invite/")));
    // In the group's repo in Bob's inbox, and told to his PDS, its authority.
    let rkey = uri.rsplit('/').next().unwrap();
    let record = h
        .host
        .group_record(&did, &inbox, &format!("{NS}.invite"), rkey)
        .unwrap()
        .unwrap();
    assert_eq!(
        (record["by"].clone(), record["message"].clone()),
        (json!(alice.did), json!("Ride with us"))
    );
    assert_eq!(bob.told.writes.lock().unwrap()[0].1["space"], inbox);
    // Asked again, the same one.
    assert_eq!(
        call(
            &h,
            &alice,
            "createInvite",
            json!({"group": did, "invitee": bob.did})
        )
        .await
        .1["uri"],
        uri
    );

    // Listed for those who invite, newest first, a page at a time.
    let soon = "2099-01-01T00:00:00.5Z";
    call(
        &h,
        &alice,
        "createInvite",
        json!({"group": did, "invitee": carol.did, "expiresAt": soon}),
    )
    .await;
    let (_, page) = ask(&h, &alice, "listInvites", &format!("group={did}&limit=1")).await;
    assert_eq!(
        (
            page["invites"][0]["invitee"].clone(),
            page["invites"][0]["expiresAt"].clone()
        ),
        (json!(carol.did), json!("2099-01-01T00:00:00.500Z"))
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
    assert_eq!(mine["invites"][0]["by"], alice.did);
    let (_, filtered) = ask(&h, &bob, "getActorInvites", "group=did:plc:other").await;
    assert_eq!(filtered["invites"], json!([]));

    // A request of theirs from before, gone once they are in.
    h.host.store.ask(&did, &bob.did, None, 0).unwrap();
    // Redeemed by asking to join: admitted, though the group invites only,
    // and the invite taken back.
    assert_eq!(
        call(&h, &bob, "requestJoin", json!({"group": did})).await,
        (200, json!({"status": "admitted"}))
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

    for (who, body, refused) in [
        (
            &alice,
            json!({"group": did, "invitee": bob.did}),
            (400, "AlreadyMember"),
        ),
        (
            &alice,
            json!({"group": did, "invitee": "bob"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "invitee": carol.did, "expiresAt": "soon"}),
            (400, "InvalidRequest"),
        ),
        (
            &bob,
            json!({"group": did, "invitee": carol.did}),
            (403, "Forbidden"),
        ),
    ] {
        let said = call(&h, who, "createInvite", body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
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
async fn invites_are_taken_back_turned_down_or_lapse() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "joinPolicy": "invite"}),
    )
    .await;
    // A face, for the invitee's list.
    let picture = h.host.upload_blob(&did, b"face", "image/png").unwrap();
    h.host
        .space_write(&did, &meta_space(&did), &[Write::Update { collection: format!("{NS}.profile"), rkey: "self".into(), value: json!({"$type": format!("{NS}.profile"), "displayName": "Riders", "joinPolicy": "invite", "avatar": picture}) }])
        .await
        .unwrap();
    let (_, made) = call(
        &h,
        &alice,
        "createInvite",
        json!({"group": did, "invitee": bob.did}),
    )
    .await;
    let uri = made["uri"].as_str().unwrap().to_string();
    assert_eq!(
        ask(&h, &bob, "getActorInvites", "").await.1["invites"][0]["group"]["avatar"],
        picture["ref"]["$link"]
    );
    assert_eq!(
        call(
            &h,
            &alice,
            "revokeInvite",
            json!({"group": did, "uri": uri})
        )
        .await,
        (200, json!({}))
    );
    assert_eq!(
        error(
            &call(
                &h,
                &alice,
                "revokeInvite",
                json!({"group": did, "uri": uri})
            )
            .await
        ),
        (400, "InviteNotFound")
    );
    assert_eq!(
        error(
            &call(
                &h,
                &alice,
                "revokeInvite",
                json!({"group": did, "uri": "nope"})
            )
            .await
        ),
        (400, "InvalidRequest")
    );
    assert_eq!(
        error(&call(&h, &alice, "revokeInvite", json!({"group": did})).await),
        (400, "InvalidRequest")
    );
    assert_eq!(
        error(&call(&h, &bob, "requestJoin", json!({"group": did})).await),
        (400, "InviteRequired")
    );

    // Turned down by its invitee.
    call(
        &h,
        &alice,
        "createInvite",
        json!({"group": did, "invitee": bob.did}),
    )
    .await;
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

    // Lapsed, it is no invite.
    call(
        &h,
        &alice,
        "createInvite",
        json!({"group": did, "invitee": carol.did, "expiresAt": "2000-01-01T00:00:00Z"}),
    )
    .await;
    assert_eq!(
        ask(&h, &alice, "listInvites", &format!("group={did}"))
            .await
            .1["invites"],
        json!([])
    );
    assert_eq!(
        error(&call(&h, &carol, "requestJoin", json!({"group": did})).await),
        (400, "InviteRequired")
    );
    // Invited again, a fresh invite, which admits.
    let (_, again) = call(
        &h,
        &alice,
        "createInvite",
        json!({"group": did, "invitee": carol.did}),
    )
    .await;
    let listed = ask(&h, &alice, "listInvites", &format!("group={did}"))
        .await
        .1;
    assert_eq!(listed["invites"][0]["uri"], again["uri"]);
    assert_eq!(
        call(&h, &carol, "requestJoin", json!({"group": did}))
            .await
            .1["status"],
        "admitted"
    );

    // A group switched off shows no invites.
    let dave = authority(&h, false, true).await;
    call(
        &h,
        &alice,
        "createInvite",
        json!({"group": did, "invitee": dave.did}),
    )
    .await;
    h.host.deactivate_account(&did, &alice.did, &[]).unwrap();
    assert_eq!(
        ask(&h, &dave, "getActorInvites", "").await.1["invites"],
        json!([])
    );
}

async fn check(
    h: &Running,
    asker: &Authority,
    space: &str,
    user: &str,
    access: &str,
) -> (u16, Json) {
    let lxm = "com.atproto.managedspace.checkUserAccess";
    get(
        h,
        lxm,
        &format!("space={space}&user={user}&access={access}"),
        &[auth(h, asker, lxm)],
    )
    .await
}

#[tokio::test]
async fn an_inbox_is_its_owners_to_read_and_groups_here_to_write() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let inbox = inbox_of(&bob.did);
    let yes = (200, json!({"authorized": true}));
    let no = (200, json!({"authorized": false}));
    assert_eq!(check(&h, &bob, &inbox, &did, "write").await, yes);
    assert_eq!(check(&h, &bob, &inbox, &carol.did, "write").await, no);
    let plain = h.host.create_account("plain.groups.test").await.unwrap();
    assert_eq!(check(&h, &bob, &inbox, &plain, "write").await, no);
    assert_eq!(check(&h, &bob, &inbox, &bob.did, "read").await, yes);
    assert_eq!(check(&h, &bob, &inbox, &did, "read").await, no);
    // A space that is no inbox, this host manages nothing in.
    let other = format!("at://{}/space/xyz.app.board/self", bob.did);
    assert_eq!(check(&h, &bob, &other, &did, "write").await, no);
    // Only its authority asks.
    assert_eq!(
        error(&check(&h, &carol, &inbox, &did, "write").await),
        (403, "Forbidden")
    );
    assert_eq!(
        error(&check(&h, &bob, "nope", &did, "write").await),
        (400, "InvalidRequest")
    );
    let lxm = "com.atproto.managedspace.checkUserAccess";
    let (status, _) = get(
        &h,
        lxm,
        &format!("space={inbox}&user={did}&access=write"),
        &[],
    )
    .await;
    assert_eq!(status, 401);
}

#[test]
fn reads_a_datetime() {
    assert_eq!(millis_of("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(millis_of("2000-03-01T00:00:00.25Z"), Some(951_868_800_250));
    assert_eq!(millis_of("2026-11-03T12:30:15Z"), Some(1_793_709_015_000));
    assert_eq!(millis_of("2024-02-29T00:00:00Z"), Some(1_709_164_800_000));
    for bad in [
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
