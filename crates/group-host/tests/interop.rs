//! The host against the reference network (`harness/pds.mjs`): a real PLC
//! directory, and on the space alpha, a real member's PDS.

mod common;

use atproto::did;
use atproto::space::{
    COMMIT_MAC, LtHash, RECORD_MAC, accept_signature, check_record_mac, element, encapsulate,
    read_repo_star, read_space_meta, read_star, star,
};
use atproto::space_auth::{Expect, Kind, sign_request, space_host_aud, verify};
use atproto::{Curve, Keypair};
use common::net::net;
use common::spaces::closed;
use common::{start, start_reachable_by_name};
use group_host::repo::Write;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[tokio::test]
async fn a_group_on_the_reference_network() {
    let net = net();
    let h = start(&[("PLC_URL", net.plc.as_str())], false).await;
    let group = h.host.create_account("riders.groups.test").await.unwrap();

    // The reference directory took the group's genesis, signed with the
    // host's rotation key, and serves its document.
    let http = reqwest::Client::new();
    let doc: Value = http
        .get(format!("{}/{group}", net.plc))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(did::pds(&doc).as_deref(), Some("https://host.test"));
    assert_eq!(did::handle(&doc).as_deref(), Some("riders.groups.test"));
    let account = h.host.store.account(&group).unwrap().unwrap();
    assert_eq!(did::signing_key(&doc).unwrap(), account.signing.public());
    assert_eq!(did::kem_key(&doc).unwrap(), account.kem.public());
    if !net.spaces {
        eprintln!("the reference PDS has no spaces: GROUP_HOST_DEV_ENV names the alpha's build");
        return;
    }

    // A member asks their own PDS for a delegation token to the group's
    // members space, and the token is as the host will check it.
    let alice = &net.accounts[0];
    let session: Value = http
        .post(format!("{}/xrpc/com.atproto.server.createSession", net.pds))
        .json(&json!({"identifier": alice.handle, "password": alice.password}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let space = format!("at:space/{group}/org.atgrp.members/self");
    let res = http
        .get(format!(
            "{}/xrpc/com.atproto.space.getDelegationToken",
            net.pds
        ))
        .query(&[("space", space.as_str())])
        .bearer_auth(session["accessJwt"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    let status = res.status();
    let body: Value = res.json().await.unwrap();
    assert!(status.is_success(), "{status} {body}");
    let theirs: Value = http
        .get(format!("{}/{}", net.plc, alice.did))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let key = did::signing_key(&theirs).unwrap();
    let now = group_host::time::now_millis() as u64 / 1000;
    let aud = space_host_aud(&group);
    let expect = Expect {
        aud: Some(&aud),
        sub: Some(&space),
    };
    let token = verify(
        Kind::Delegation,
        body["token"].as_str().unwrap(),
        &key,
        &expect,
        now,
    )
    .unwrap();
    assert_eq!(token.iss, alice.did);
}

/// What `POST nsid` with `body` answers, as `bearer`.
async fn call(url: &str, nsid: &str, bearer: &str, body: &Value) -> Value {
    let res = reqwest::Client::new()
        .post(format!("{url}/xrpc/{nsid}"))
        .bearer_auth(bearer)
        .json(body)
        .send()
        .await
        .unwrap();
    let status = res.status();
    let body: Value = res.json().await.unwrap_or(Value::Null);
    assert!(status.is_success(), "{nsid}: {status} {body}");
    body
}

#[tokio::test]
async fn a_groups_repo_in_a_space_of_the_reference_pds() {
    let net = net();
    if !net.spaces {
        eprintln!("the reference PDS has no spaces: GROUP_HOST_DEV_ENV names the alpha's build");
        return;
    }
    // On http, as on a network of test servers: the reference PDS is on
    // this machine.
    let vars = [
        ("PLC_URL", net.plc.as_str()),
        ("PUBLIC_URL", "http://host.test"),
    ];
    let h = start(&vars, false).await;
    let group = h.host.create_account("climbers.groups.test").await.unwrap();
    let http = reqwest::Client::new();

    // Bob, on the reference PDS, makes a space of his that the group
    // writes to and he reads.
    let bob = &net.accounts[1];
    let session = call(
        &net.pds,
        "com.atproto.server.createSession",
        "",
        &json!({"identifier": bob.handle, "password": bob.password}),
    )
    .await;
    let jwt = session["accessJwt"].as_str().unwrap();
    let created = call(
        &net.pds,
        "com.atproto.space.createSpace",
        jwt,
        &json!({
            "spaceType": "org.atgrp.content",
            "skey": "climbs",
            "policy": {"$type": "com.atproto.memberspace.policy"},
        }),
    )
    .await;
    let space = created["space"].as_str().unwrap().to_string();
    call(
        &net.pds,
        "com.atproto.memberspace.putMembers",
        jwt,
        &json!({"space": space, "members": [
            {"did": bob.did, "read": true, "write": false},
            {"did": group, "read": false, "write": true},
        ]}),
    )
    .await;

    // The group writes there, and its host tells Bob's PDS.
    let rule =
        json!({"$type": "org.atgrp.rule", "title": "Clip in", "text": "Clip in before the climb."});
    let commit = h
        .host
        .space_write(
            &group,
            &space,
            &[Write::Create {
                collection: "org.atgrp.rule".into(),
                rkey: "one".into(),
                value: rule.clone(),
            }],
        )
        .await
        .unwrap();

    // Bob's PDS issues him a credential to the space, for a key of his.
    let token = http
        .get(format!(
            "{}/xrpc/com.atproto.space.getDelegationToken",
            net.pds
        ))
        .query(&[("space", space.as_str())])
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let key = Keypair::generate(Curve::P256);
    let issued = call(
        &net.pds,
        "com.atproto.space.authorizeSpaceSync",
        "",
        &json!({
            "space": space,
            "credentialKey": key.public().did_key(),
            "delegationToken": token["token"],
        }),
    )
    .await;
    let authorization = format!(
        "Atproto-Space-Credential {}",
        issued["credential"].as_str().unwrap()
    );

    // The authority heard of the write: the group's repo there is at the
    // commit the host made.
    let mut req = http
        .get(format!("{}/xrpc/com.atproto.space.listWriters", net.pds))
        .query(&[("space", space.as_str())]);
    for (k, v) in sign_request(&key, &authorization, &bob.did).unwrap() {
        req = req.header(k, v);
    }
    let res = req.send().await.unwrap();
    let status = res.status();
    let repos: Value = res.json().await.unwrap();
    assert!(status.is_success(), "{status} {repos}");
    let repos = repos["repos"].as_array().unwrap();
    let ours = repos.iter().find(|r| r["repo"] == group.as_str()).unwrap();
    assert_eq!(ours["$type"], "com.atproto.space.defs#repoWrite");
    assert_eq!(ours["repoRev"], commit.rev.as_str());
    assert_eq!(
        ours["data"],
        atproto::Value::Bytes(commit.data.to_vec()).to_json()
    );

    // And with the credential the reference issued, the host serves Bob
    // the group's repo, vouched for under the secret he agreed.
    let account = h.host.store.account(&group).unwrap().unwrap();
    let (share, secret) = encapsulate(&account.kem.public().did_key()).unwrap();
    let mut req = http
        .get(format!("{}/xrpc/com.atproto.space.getRepo", h.url))
        .query(&[("space", space.as_str()), ("repo", group.as_str())])
        .header("accept-signature", accept_signature(COMMIT_MAC, &share));
    for (k, v) in sign_request(&key, &authorization, &group).unwrap() {
        req = req.header(k, v);
    }
    let res = req.send().await.unwrap();
    assert_eq!(res.status(), 200);
    let said = res.headers().clone();
    assert_eq!(said["content-type"], "application/vnd.dasl.star");
    let bytes = res.bytes().await.unwrap().to_vec();
    let header = |n: &str| said.get(n).and_then(|v| v.to_str().ok());
    let (read, _) = read_repo_star(&bytes, &space, &group, &share, &secret, header).unwrap();
    assert_eq!(read, commit);
    let records = BTreeMap::from([(
        "org.atgrp.rule/one".to_string(),
        atproto::drisl::encode(&atproto::Value::from_json(&rule).unwrap()),
    )]);
    assert_eq!(bytes, star(&commit, &records));
}

/// What `GET nsid` with `query` and `headers` answers at `url`.
async fn get(
    url: &str,
    nsid: &str,
    query: &[(&str, &str)],
    headers: &[(&str, String)],
) -> (reqwest::StatusCode, reqwest::header::HeaderMap, Value) {
    let mut req = reqwest::Client::new()
        .get(format!("{url}/xrpc/{nsid}"))
        .query(query);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let res = req.send().await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    (status, headers, res.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn the_host_as_authority_for_a_member_on_the_reference_pds() {
    let net = net();
    if !net.spaces {
        eprintln!("the reference PDS has no spaces: GROUP_HOST_DEV_ENV names the alpha's build");
        return;
    }
    // By name: the reference PDS reaches no server at an IP address.
    let h = start_reachable_by_name(&[("PLC_URL", net.plc.as_str())]).await;
    let group = h.host.create_account("runners.groups.test").await.unwrap();
    let space = h
        .host
        .create_space(&group, "org.atgrp.content", "self", &closed())
        .unwrap();
    let alice = &net.accounts[0];
    h.host.grant(&space, &alice.did, true, true).unwrap();
    let session = call(
        &net.pds,
        "com.atproto.server.createSession",
        "",
        &json!({"identifier": alice.handle, "password": alice.password}),
    )
    .await;
    let jwt = session["accessJwt"].as_str().unwrap();

    // Alice writes in the group's space at her own PDS, which tells the
    // host, the space's authority.
    let note =
        json!({"$type": "org.atgrp.note", "text": "Hello", "createdAt": "2026-11-03T00:00:00Z"});
    let made = call(
        &net.pds,
        "com.atproto.space.createRecord",
        jwt,
        &json!({"space": space, "repo": alice.did, "collection": "org.atgrp.note", "rkey": "one", "record": note}),
    )
    .await;
    let mut writers = Vec::new();
    for _ in 0..100 {
        writers = h.host.store.writers(&space, None, 10).unwrap();
        if !writers.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        writers.len(),
        1,
        "the reference PDS never told the host of {made}"
    );
    assert_eq!(writers[0].did, alice.did);

    // The index reads her repo from her PDS, as any syncer reads a space:
    // with the group's own credential, its commit vouched for.
    h.host.sync_space(&space).await.unwrap();
    let indexed = h.host.store.indexed(&space, None, None, 10).unwrap();
    assert_eq!(indexed.len(), 1, "the index did not read her repo");
    assert_eq!(
        (indexed[0].did.as_str(), indexed[0].rkey.as_str()),
        (alice.did.as_str(), "one")
    );
    let value = atproto::drisl::decode(&indexed[0].value).unwrap().to_json();
    assert_eq!(value, note);

    // Her PDS's delegation token buys a credential from the host.
    let (status, _, token) = get(
        &net.pds,
        "com.atproto.space.getDelegationToken",
        &[("space", space.as_str())],
        &[("authorization", format!("Bearer {jwt}"))],
    )
    .await;
    assert!(status.is_success(), "{status} {token}");
    let key = Keypair::generate(Curve::P256);
    let res = reqwest::Client::new()
        .post(format!("{}/xrpc/com.atproto.space.authorizeSpaceSync", h.url))
        .json(&json!({"space": space, "credentialKey": key.public().did_key(), "delegationToken": token["token"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let issued: Value = res.json().await.unwrap();
    // The space's revision is the writer set's, which her write moved.
    let space_rev = issued["spaceRev"].as_str().unwrap();
    assert_eq!(space_rev, writers[0].space_rev);
    let authorization = format!(
        "Atproto-Space-Credential {}",
        issued["credential"].as_str().unwrap()
    );

    // Her PDS takes the host's credential, and vouches for her record.
    let theirs: Value = reqwest::Client::new()
        .get(format!("{}/{}", net.plc, alice.did))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let kem = did::kem_key(&theirs).unwrap().did_key();
    let (share, secret) = encapsulate(&kem).unwrap();
    let mut headers: Vec<(&str, String)> = sign_request(&key, &authorization, &alice.did)
        .unwrap()
        .into_iter()
        .collect();
    headers.push(("accept-signature", accept_signature(RECORD_MAC, &share)));
    let (status, said, record) = get(
        &net.pds,
        "com.atproto.space.getRecord",
        &[
            ("space", space.as_str()),
            ("repo", alice.did.as_str()),
            ("collection", "org.atgrp.note"),
            ("rkey", "one"),
        ],
        &headers,
    )
    .await;
    assert!(status.is_success(), "{status} {record}");
    assert_eq!(record["value"], note);
    let header = |n: &str| said.get(n).and_then(|v| v.to_str().ok());
    let (uri, cid) = check_record_mac(header, &share, &secret).unwrap();
    assert_eq!(
        (uri.as_str(), cid.as_str()),
        (
            record["uri"].as_str().unwrap(),
            record["cid"].as_str().unwrap()
        )
    );

    // And the host lists her repo to the same credential.
    let headers: Vec<(&str, String)> = sign_request(&key, &authorization, &group)
        .unwrap()
        .into_iter()
        .collect();
    let (status, _, repos) = get(
        &h.url,
        "com.atproto.space.listWriters",
        &[("space", space.as_str())],
        &headers,
    )
    .await;
    assert_eq!(status, 200, "{repos}");
    assert_eq!(
        repos["repos"][0]["$type"],
        "com.atproto.space.defs#repoWrite"
    );
    assert_eq!(repos["repos"][0]["repo"], alice.did);
    assert_eq!(repos["cursor"], space_rev);
}

#[tokio::test]
async fn an_invite_into_an_inbox_on_the_reference_pds() {
    let net = net();
    if !net.spaces {
        eprintln!("the reference PDS has no spaces: GROUP_HOST_DEV_ENV names the alpha's build");
        return;
    }
    let h = start_reachable_by_name(&[("PLC_URL", net.plc.as_str())]).await;
    let service = h.host.config.service_did.clone();
    assert!(service.starts_with("did:web:localhost"), "{service}");

    // Alice's inbox, at her PDS, as her app makes it: a memberspace that
    // tracks anyone's writes and names no members, so groups write in it
    // and only she reads it.
    let alice = &net.accounts[0];
    let session = call(
        &net.pds,
        "com.atproto.server.createSession",
        "",
        &json!({"identifier": alice.handle, "password": alice.password}),
    )
    .await;
    let jwt = session["accessJwt"].as_str().unwrap();
    let created = call(
        &net.pds,
        "com.atproto.space.createSpace",
        jwt,
        &json!({
            "spaceType": "org.atgrp.inbox",
            "skey": "self",
            "policy": {"$type": "com.atproto.memberspace.policy", "publicWrite": true},
        }),
    )
    .await;
    let inbox = created["space"].as_str().unwrap().to_string();

    // A group here invites her: its write is told to her PDS, which takes
    // it, as the inbox takes anyone's.
    let founder = common::spaces::authority(&h, false, true).await;
    let group = common::groups::start_group(
        &h,
        &founder,
        json!({"handle": "riders.groups.test", "displayName": "Riders"}),
    )
    .await;
    let invite = json!({
        "collection": "org.atgrp.invite",
        "record": {"$type": "org.atgrp.invite", "createdAt": "2026-11-03T00:00:00Z"},
    });
    let create = "com.atproto.space.createRecord";
    let made = common::oauth::inbox_write(&h, &founder, &group, &inbox, create, invite).await;
    assert_eq!(made.status, 200, "{}", made.body);
    // Nothing is owed: her PDS took the write.
    let later = group_host::time::now_millis() / 1000 + 3600;
    assert_eq!(
        h.host.store.due(later, 10).unwrap(),
        vec![],
        "her PDS refused the invite"
    );

    // And it keeps the group among her inbox's writers.
    let (_, _, token) = get(
        &net.pds,
        "com.atproto.space.getDelegationToken",
        &[("space", inbox.as_str())],
        &[("authorization", format!("Bearer {jwt}"))],
    )
    .await;
    let key = Keypair::generate(Curve::P256);
    let issued = call(
        &net.pds,
        "com.atproto.space.authorizeSpaceSync",
        "",
        &json!({"space": inbox, "credentialKey": key.public().did_key(), "delegationToken": token["token"]}),
    )
    .await;
    let authorization = format!(
        "Atproto-Space-Credential {}",
        issued["credential"].as_str().unwrap()
    );
    let headers: Vec<(&str, String)> = sign_request(&key, &authorization, &alice.did)
        .unwrap()
        .into_iter()
        .collect();
    let (status, _, repos) = get(
        &net.pds,
        "com.atproto.space.listWriters",
        &[("space", inbox.as_str())],
        &headers,
    )
    .await;
    assert!(status.is_success(), "{status} {repos}");
    let writers: Vec<&str> = repos["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["repo"].as_str().unwrap())
        .collect();
    assert!(writers.contains(&group.as_str()), "{repos}");
}

#[tokio::test]
async fn reads_the_archives_the_reference_pds_exports_of_a_space() {
    let net = net();
    if !net.spaces {
        eprintln!("the reference PDS has no spaces: GROUP_HOST_DEV_ENV names the alpha's build");
        return;
    }
    // Alice, on the reference PDS, governs a space Bob reads and she writes in.
    let (alice, bob) = (&net.accounts[0], &net.accounts[1]);
    let session = call(
        &net.pds,
        "com.atproto.server.createSession",
        "",
        &json!({"identifier": alice.handle, "password": alice.password}),
    )
    .await;
    let jwt = session["accessJwt"].as_str().unwrap();
    let created = call(
        &net.pds,
        "com.atproto.space.createSpace",
        jwt,
        &json!({
            "spaceType": "com.example.forum",
            "skey": "moving",
            "policy": {"$type": "com.atproto.memberspace.policy", "publicRead": true},
        }),
    )
    .await;
    let space = created["space"].as_str().unwrap().to_string();
    call(
        &net.pds,
        "com.atproto.memberspace.putMembers",
        jwt,
        &json!({"space": space, "members": [{"did": bob.did, "read": true, "write": false}]}),
    )
    .await;
    let note = json!({"$type": "com.example.note", "text": "moving soon", "createdAt": "2026-11-04T00:00:00Z"});
    call(
        &net.pds,
        "com.atproto.space.createRecord",
        jwt,
        &json!({"space": space, "repo": alice.did, "collection": "com.example.note", "rkey": "one", "record": note}),
    )
    .await;

    // What it exports as she moves: the space as its authority keeps it...
    let http = reqwest::Client::new();
    let export = async |nsid: &str, query: &[(&str, &str)]| {
        let res = http
            .get(format!("{}/xrpc/com.atproto.space.{nsid}", net.pds))
            .query(query)
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success(), "{nsid}: {}", res.status());
        res.bytes().await.unwrap().to_vec()
    };
    let meta = read_space_meta(&export("exportSpaceMeta", &[("space", &space)]).await).unwrap();
    assert_eq!(meta.space, space);
    let policy = atproto::Value::Map(meta.policy).to_json();
    assert_eq!(policy["$type"], "com.atproto.memberspace.policy");
    assert_eq!(policy["publicRead"], true);
    let members: Vec<(String, Value)> = meta
        .policy_state
        .into_iter()
        .map(|(k, v)| (k, atproto::Value::Map(v).to_json()))
        .collect();
    let bob_key = format!("com.atproto.memberspace.member/{}", bob.did);
    let bob_member = members.iter().find(|(k, _)| *k == bob_key).unwrap();
    assert_eq!(
        (&bob_member.1["read"], &bob_member.1["write"]),
        (&json!(true), &json!(false))
    );
    let writer = meta.writers.iter().find(|w| w.did == alice.did).unwrap();

    // ...and her repo there, whole, as the writer set last heard of it.
    let repo = export("getRepo", &[("space", &space), ("repo", &alice.did)]).await;
    let (commit, records) = read_star(&repo).unwrap();
    assert_eq!(
        (commit.repo.as_str(), commit.space.as_str()),
        (alice.did.as_str(), space.as_str())
    );
    assert_eq!(writer.data, commit.data.to_vec());
    assert_eq!(writer.repo_rev, commit.rev);
    let mut hash = LtHash::default();
    for (path, value) in &records {
        let (collection, rkey) = path.split_once('/').unwrap();
        hash.add(&element(collection, rkey, &atproto::Cid::of_cbor(value)));
    }
    assert_eq!(hash.digest(), commit.data);
    assert_eq!(records.keys().collect::<Vec<_>>(), ["com.example.note/one"]);
}
