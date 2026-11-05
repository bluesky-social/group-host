//! Groups: started by someone signed in elsewhere, with their two spaces
//! and what those hold from the first day; read back as each viewer may
//! see them, and listed.

mod common;

use atproto::{Curve, Keypair};
use common::groups::*;
use common::spaces::*;
use common::start;
use group_host::authority::Access;
use group_host::group::{members_space, meta_space};
use group_host::rbac::ACTIONS;
use group_host::repo::Write;
use serde_json::{Value as Json, json};

#[tokio::test]
async fn starts_a_group_with_its_spaces() {
    let h = start(&[], false).await;
    let [alice, bob, _] = people(&h).await;
    let lxm = format!("{NS}.createGroup");
    let body = json!({
        "handle": RIDERS,
        "displayName": "Riders",
        "description": "We ride.",
        "joinPolicy": "approval",
        "stewards": [{"did": bob.did}, {"did": alice.did}],
    });
    let (status, made) = post(&h, &lxm, &body, &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(status, 200, "{made}");
    let did = made["did"].as_str().unwrap();
    assert_eq!(made["meta"], meta_space(did));
    assert_eq!(made["members"], members_space(did));

    let record = |space: &str, collection: &str, rkey: &str| {
        h.host
            .group_record(did, space, &format!("{NS}.{collection}"), rkey)
            .unwrap()
            .unwrap()
    };
    let (meta, members) = (meta_space(did), members_space(did));
    let profile = record(&meta, "profile", "self");
    assert_eq!(profile["displayName"], "Riders");
    assert_eq!(profile["joinPolicy"], "approval");
    // Meta is public, its profile and rules written by admins; members
    // read the members space.
    assert_eq!(
        record(&meta, "access", "self"),
        json!({
            "$type": format!("{NS}.access"),
            "public": true,
            "grants": [{"role": "admin", "scopes": [format!("space:{NS}.meta?collection={NS}.profile&collection={NS}.rule")]}],
        })
    );
    assert_eq!(
        record(&meta, "labeler", "self")["subjectTypes"],
        json!(["account", "record"])
    );
    assert_eq!(
        record(&members, "access", "self")["grants"],
        json!([{"role": "member", "scopes": [format!("space:{NS}.members?action=read")]}])
    );
    assert_eq!(record(&members, "role", "member")["default"], true);
    // The founder and stewards run it from the start.
    for who in [&alice, &bob] {
        assert_eq!(
            h.host.roles(did, &who.did).unwrap(),
            ["admin", "member"],
            "{}",
            who.did
        );
    }
    let indexed = h
        .host
        .group_records(did, &members, &format!("{NS}.space"))
        .unwrap();
    let spaces: Vec<&str> = indexed
        .iter()
        .map(|(_, s)| s["space"].as_str().unwrap())
        .collect();
    assert_eq!(spaces, [meta.as_str(), members.as_str()]);
    // Listed, for a meta anyone may read.
    let (_, declared) = get(
        &h,
        "com.atproto.repo.getRecord",
        &format!("repo={did}&collection={NS}.declaration&rkey=self"),
        &[],
    )
    .await;
    assert_eq!(declared["value"]["meta"], meta);
    // Both spaces are its, as their authority.
    assert!(h.host.store.space(&meta).unwrap().is_some());
    assert!(h.host.store.space(&members).unwrap().is_some());
}

#[tokio::test]
async fn shows_a_group_as_each_viewer_may_see_it() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "stewards": [{"did": bob.did}]}),
    )
    .await;
    let meta = meta_space(&did);
    let picture = h.host.upload_blob(&did, b"face", "image/png").unwrap();
    h.host
        .space_write(
            &did,
            &meta,
            &[
                Write::Update {
                    collection: format!("{NS}.profile"),
                    rkey: "self".into(),
                    value: json!({"$type": format!("{NS}.profile"), "displayName": "Riders", "joinPolicy": "open", "avatar": picture, "topics": [{"topic": "bikes"}]}),
                },
                rule_at("b", "Second", None, "2026-11-02T00:00:00Z"),
                rule_at("a", "Third", None, "2026-11-03T00:00:00Z"),
                rule_at("c", "First", Some(1), "2026-11-04T00:00:00Z"),
            ],
        )
        .await
        .unwrap();

    // The founder: everything, and every action.
    let mine = view(&h, &did, Some(&alice)).await;
    let group = &mine["group"];
    assert_eq!(group["handle"], RIDERS);
    assert_eq!(group["address"], RIDERS);
    assert_eq!(group["displayName"], "Riders");
    assert_eq!(group["joinPolicy"], "open");
    assert_eq!(group["avatar"], picture["ref"]["$link"]);
    assert_eq!(group["topics"], json!([{"topic": "bikes"}]));
    let titles: Vec<&str> = group["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["First", "Second", "Third"]);
    let viewer = &group["viewer"];
    assert_eq!(viewer["member"], true);
    assert_eq!(
        viewer["roles"],
        json!([{"role": "admin"}, {"role": "member"}])
    );
    let mut every: Vec<&str> = ACTIONS.to_vec();
    every.sort();
    assert_eq!(viewer["actions"], json!(every));
    assert_eq!(viewer["accepted"], false);
    let members: Vec<&str> = mine["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["actor"]["did"].as_str().unwrap())
        .collect();
    assert_eq!(members.len(), 2);
    assert!(members.contains(&alice.did.as_str()) && members.contains(&bob.did.as_str()));
    // Each by the handle their DID document names.
    assert!(
        mine["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["actor"]["handle"] == "authority.test")
    );
    assert_eq!(
        mine["roles"],
        json!([
            {"key": "admin", "name": "Admin", "description": "Runs the group."},
            {"key": "member", "name": "Member"},
        ])
    );
    let admin = &mine["bindings"][0];
    assert_eq!(admin["role"], "admin");
    assert_eq!(admin["actions"], json!(every));
    assert_eq!(admin["assignable"], json!([{"role": "member"}]));
    assert_eq!(admin["repoCollections"], json!([{"collection": "*"}]));
    assert_eq!(mine["bindings"][1]["actions"], json!([]));
    assert_eq!(mine["spaces"][0]["uri"], meta);
    assert_eq!(mine["spaces"][0]["spaceType"], format!("{NS}.meta"));
    assert_eq!(mine["spaces"][0]["public"], true);
    assert!(mine["spaces"][0]["grants"].is_array());
    assert_eq!(mine["spaces"][1].get("public"), None);
    assert_eq!(mine["spaces"][1]["grants"][0]["role"], "member");

    // A member who is no admin is shown a space only admins read, by its
    // name, but not who reads it.
    let admins = h
        .host
        .create_space(&did, "fyi.opensocial.board", "self", false)
        .unwrap();
    h.host
        .space_write(
            &did,
            &admins,
            &[Write::Create {
                collection: format!("{NS}.access"),
                rkey: "self".into(),
                value: json!({"$type": format!("{NS}.access"), "grants": [{"role": "admin", "scopes": ["space:*?action=read"]}]}),
            }],
        )
        .await
        .unwrap();
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[
                Write::Create {
                    collection: format!("{NS}.space"),
                    rkey: "3l5mz5mkbn22z".into(),
                    value: json!({"$type": format!("{NS}.space"), "space": admins, "name": "Board", "createdAt": "2026-11-03T00:00:00Z"}),
                },
                Write::Create {
                    collection: format!("{NS}.membership"),
                    rkey: carol.did.clone(),
                    value: json!({"$type": format!("{NS}.membership"), "member": carol.did, "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}),
                },
            ],
        )
        .await
        .unwrap();
    let hers = view(&h, &did, Some(&carol)).await;
    let spaces = hers["spaces"].as_array().unwrap();
    let board = spaces.iter().find(|s| s["uri"] == admins).unwrap();
    assert_eq!(board["name"], "Board");
    assert_eq!(board.get("grants"), None);
    assert_eq!(hers["group"]["viewer"]["actions"], json!([]));
    let theirs = spaces
        .iter()
        .find(|s| s["uri"] == members_space(&did))
        .unwrap();
    assert_eq!(theirs["grants"][0]["role"], "member");

    // Once they write their acceptance, they have accepted.
    h.host
        .store
        .record_writer(
            &members_space(&did),
            &alice.did,
            "3l5mz5mkbn22b",
            &[7; 32],
            "3l5mz5mkbn22b",
        )
        .unwrap();
    assert_eq!(
        view(&h, &did, Some(&alice)).await["group"]["viewer"]["accepted"],
        true
    );

    // Someone else signed in: the public face, and no standing.
    let dave = authority(&h, false, true).await;
    let theirs = view(&h, &did, Some(&dave)).await;
    assert_eq!(theirs["group"]["displayName"], "Riders");
    assert_eq!(
        theirs["group"]["viewer"],
        json!({"member": false, "roles": [], "actions": []})
    );
    assert_eq!(theirs.get("members"), None);
    // Nobody signed in: the public face alone.
    let anyone = view(&h, &did, None).await;
    assert_eq!(anyone["group"]["displayName"], "Riders");
    assert_eq!(anyone["group"].get("viewer"), None);
}

fn rule_at(rkey: &str, title: &str, order: Option<i64>, at: &str) -> Write {
    let mut value =
        json!({"$type": format!("{NS}.rule"), "title": title, "text": "", "createdAt": at});
    if let Some(order) = order {
        value["order"] = json!(order);
    }
    Write::Create {
        collection: format!("{NS}.rule"),
        rkey: rkey.into(),
        value,
    }
}

#[tokio::test]
async fn a_group_for_its_members_shows_strangers_nothing() {
    let h = start(&[], false).await;
    let [alice, _, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "metaReadableBy": [{"role": "member"}], "joinPolicy": "invite"}),
    )
    .await;
    let theirs = view(&h, &did, Some(&carol)).await;
    assert_eq!(theirs["group"].get("displayName"), None);
    assert_eq!(theirs["group"].get("rules"), None);
    let mine = view(&h, &did, Some(&alice)).await;
    assert_eq!(mine["group"]["joinPolicy"], "invite");
    // Not listed: no declaration.
    let (status, _) = get(
        &h,
        "com.atproto.repo.getRecord",
        &format!("repo={did}&collection={NS}.declaration&rkey=self"),
        &[],
    )
    .await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn its_access_records_say_who_reads_and_writes_its_spaces() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "stewards": [{"did": bob.did}]}),
    )
    .await;
    let (meta, members) = (meta_space(&did), members_space(&did));
    let may = |space: &str, who: &str, access| {
        let hosted = h.host.store.space(space).unwrap().unwrap();
        h.host.may(space, &hosted, Some(who), access).unwrap()
    };
    assert!(may(&members, &alice.did, Access::Read));
    assert!(may(&members, &bob.did, Access::Write));
    assert!(!may(&members, &carol.did, Access::Read));
    assert!(!may(&members, &carol.did, Access::Write));
    assert!(may(&meta, &carol.did, Access::Read));

    // So a credential to the members space is for its members.
    let ask = |who: &Authority, space: &str| {
        let aud = format!("{did}#atproto_pds");
        let make = atproto::space_auth::Make {
            iss: &who.did,
            sub: space,
            aud: Some(&aud),
            ..Default::default()
        };
        let token = atproto::space_auth::create(
            atproto::space_auth::Kind::Delegation,
            &make,
            &who.key,
            now(),
        )
        .unwrap();
        json!({"space": space, "credentialKid": Keypair::generate(Curve::P256).public().did_key(), "delegationToken": token})
    };
    let (status, _) = post(
        &h,
        "com.atproto.space.getSpaceCredential",
        &ask(&alice, &members),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    let refused = post(
        &h,
        "com.atproto.space.getSpaceCredential",
        &ask(&carol, &members),
        &[],
    )
    .await;
    assert_eq!(refused.1["error"], "SpaceNotFound");
    // A token refused is not spent: it is no good to anyone else anyway.
    let carols = ask(&carol, &meta);
    assert_eq!(
        post(&h, "com.atproto.space.getSpaceCredential", &carols, &[])
            .await
            .0,
        200
    );
}

#[tokio::test]
async fn lists_every_group_here() {
    let h = start(&[], false).await;
    let [alice, _, _] = people(&h).await;
    let open = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    h.host
        .space_write(
            &open,
            &meta_space(&open),
            &[rule_at("a", "Be kind", None, "2026-11-03T00:00:00Z")],
        )
        .await
        .unwrap();
    let closed = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers", "metaReadableBy": [{"role": "member"}]}),
    )
    .await;
    // An account that is no group is not listed.
    h.host.create_account("plain.groups.test").await.unwrap();
    let (status, listed) = get(&h, &format!("{NS}.listGroups"), "", &[]).await;
    assert_eq!(status, 200);
    let groups = listed["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    let of = |did: &str| groups.iter().find(|g| g["did"] == did).unwrap().clone();
    assert_eq!(of(&open)["displayName"], "Riders");
    assert_eq!(
        of(&open)["rules"],
        json!([{"rkey": "a", "title": "Be kind", "text": ""}])
    );
    assert_eq!(of(&closed).get("displayName"), None);
    assert_eq!(of(&closed)["handle"], "hikers.groups.test");
}

#[tokio::test]
async fn lists_groups_past_a_page() {
    let h = start(&[], false).await;
    for i in 0..101 {
        h.host
            .create_account(&format!("a{i:03}.groups.test"))
            .await
            .unwrap();
    }
    let (status, listed) = get(&h, &format!("{NS}.listGroups"), "", &[]).await;
    assert_eq!((status, listed), (200, json!({"groups": []})));
}

#[tokio::test]
async fn refuses_a_group_it_should_not_start() {
    let h = start(&[], false).await;
    let [alice, _, _] = people(&h).await;
    let lxm = format!("{NS}.createGroup");
    let me = [auth(&h, &alice, &lxm)];
    let body = |extra: Json| {
        let mut b = json!({"handle": RIDERS, "displayName": "Riders"});
        b.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        b
    };
    for (body, headers, refused) in [
        (body(json!({})), vec![], (401, "AuthMissing")),
        (
            body(json!({})),
            vec![auth_for(&alice, "did:web:elsewhere", &lxm)],
            (401, "BadJwtAudience"),
        ),
        (
            body(json!({})),
            vec![auth(&h, &alice, "fyi.opensocial.getGroup")],
            (401, "BadJwtLexiconMethod"),
        ),
        (
            json!({"displayName": "Riders"}),
            me.to_vec(),
            (400, "InvalidRequest"),
        ),
        (
            json!({"handle": RIDERS}),
            me.to_vec(),
            (400, "InvalidRequest"),
        ),
        (
            body(json!({"joinPolicy": "sometimes"})),
            me.to_vec(),
            (400, "InvalidRequest"),
        ),
        (
            body(json!({"stewards": [{"did": "bob"}]})),
            me.to_vec(),
            (400, "InvalidRequest"),
        ),
        // Longer than the lexicon has them.
        (
            body(json!({"displayName": "x".repeat(65)})),
            me.to_vec(),
            (400, "InvalidRequest"),
        ),
        (
            body(json!({"description": "x".repeat(2001)})),
            me.to_vec(),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = post(&h, &lxm, &body, &headers).await;
        assert_eq!(
            (said.0, said.1["error"].as_str().unwrap()),
            refused,
            "{body}"
        );
    }
    // A token for one of this host's services is for it.
    let service = format!("{}#opensocial", h.host.config.service_did);
    let said = post(
        &h,
        &lxm,
        &body(json!({"joinPolicy": "open"})),
        &[auth_for(&alice, &service, &lxm)],
    )
    .await;
    assert_eq!(said.0, 200, "{}", said.1);
    let taken = post(&h, &lxm, &body(json!({})), &me).await;
    assert_eq!(
        (taken.0, taken.1["error"].as_str().unwrap()),
        (400, "HandleTaken")
    );
    // A handle the host refuses for another reason says so.
    let bad = post(
        &h,
        &lxm,
        &body(json!({"handle": "riders.elsewhere.test"})),
        &me,
    )
    .await;
    assert_eq!(bad.1["error"], "InvalidHandle");

    // A host that names who may start groups starts none for anyone else.
    let picky = start(&[("GROUP_FOUNDERS", "did:plc:someone")], false).await;
    let [dave, _, _] = people(&picky).await;
    let said = post(&picky, &lxm, &body(json!({})), &[auth(&picky, &dave, &lxm)]).await;
    assert_eq!(
        (said.0, said.1["error"].as_str().unwrap()),
        (403, "NotAllowed")
    );
    assert!(picky.host.may_found("did:plc:someone"));

    // A group that is not one is not found.
    let (status, said) = get(&h, &format!("{NS}.getGroup"), "group=did:plc:nobody", &[]).await;
    assert_eq!(
        (status, said["error"].as_str().unwrap()),
        (400, "GroupNotFound")
    );
    let plain = h.host.create_account("plain.groups.test").await.unwrap();
    let (status, _) = get(
        &h,
        &format!("{NS}.getGroup"),
        &format!("group={plain}"),
        &[],
    )
    .await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn keeps_a_groups_address_for_it_after_it_takes_a_handle_of_its_own() {
    let h = start(&[("HANDLE_DOMAIN", ".groups.test")], false).await;
    let [alice, bob, _] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let free = async |address: &str| {
        h.get("fyi.opensocial.checkAddress", &format!("address={address}"))
            .await
    };
    assert_eq!(free("hikers.groups.test").await.1["available"], true);
    assert_eq!(
        free("Riders.groups.test").await.1,
        json!({"address": RIDERS, "available": false})
    );

    // On a domain of its own, its address stays its own.
    h.host
        .store
        .set_handle(&did, "riders.example.com", &[])
        .unwrap();
    assert_eq!(free(RIDERS).await.1["available"], false);
    let lxm = format!("{NS}.createGroup");
    let body = json!({"handle": RIDERS, "displayName": "Riders again"});
    let (status, said) = post(&h, &lxm, &body, &[auth(&h, &bob, &lxm)]).await;
    assert_eq!(
        (status, said["error"].as_str()),
        (400, Some("HandleTaken")),
        "{said}"
    );

    // Only one label under this host's domain is an address here.
    for not_ours in ["riders.example.com", ".groups.test", "a.b.groups.test"] {
        assert_eq!(free(not_ours).await.0, 400, "{not_ours}");
    }
}
