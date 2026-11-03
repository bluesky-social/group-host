//! The host against the reference network (`harness/pds.mjs`): a real PLC
//! directory, and on the space alpha, a real member's PDS.

mod common;

use atproto::did;
use atproto::space::{COMMIT_MAC, encapsulate, mac_header, star, want_mac_header};
use atproto::space_auth::{Expect, Kind, sign_request, space_host_aud, verify};
use atproto::{Curve, Keypair};
use common::net::net;
use common::start;
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
    let space = format!("at://{group}/space/fyi.opensocial.members/self");
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
    let h = start(&[("PLC_URL", net.plc.as_str())], false).await;
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
            "spaceType": "fyi.opensocial.content",
            "skey": "climbs",
            "policy": {"$type": "com.atproto.memberspace.policy"},
        }),
    )
    .await;
    let space = created["uri"].as_str().unwrap().to_string();
    for (did, read, write) in [
        (bob.did.as_str(), true, false),
        (group.as_str(), false, true),
    ] {
        call(
            &net.pds,
            "com.atproto.memberspace.putMember",
            jwt,
            &json!({"space": space, "did": did, "read": read, "write": write}),
        )
        .await;
    }

    // The group writes there, and its host tells Bob's PDS.
    let rule = json!({"$type": "fyi.opensocial.rule", "title": "Clip in", "createdAt": "2026-11-03T00:00:00Z"});
    let commit = h
        .host
        .space_write(
            &group,
            &space,
            &[Write::Create {
                collection: "fyi.opensocial.rule".into(),
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
        "com.atproto.space.getSpaceCredential",
        "",
        &json!({
            "space": space,
            "credentialKid": key.public().did_key(),
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
        .get(format!("{}/xrpc/com.atproto.space.listRepos", net.pds))
        .query(&[("space", space.as_str())]);
    for (k, v) in sign_request(&key, &authorization, &bob.did).unwrap() {
        req = req.header(k, v);
    }
    let res = req.send().await.unwrap();
    let status = res.status();
    let repos: Value = res.json().await.unwrap();
    assert!(status.is_success(), "{status} {repos}");
    let repos = repos["repos"].as_array().unwrap();
    let ours = repos.iter().find(|r| r["did"] == group.as_str()).unwrap();
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
        .header(
            "atproto-space-want-mac",
            want_mac_header(COMMIT_MAC, &share),
        );
    for (k, v) in sign_request(&key, &authorization, &group).unwrap() {
        req = req.header(k, v);
    }
    let res = req.send().await.unwrap();
    assert_eq!(res.status(), 200);
    let mac = res.headers()["atproto-space-mac"]
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(mac, mac_header(COMMIT_MAC, &commit.mac(&secret)));
    let records = BTreeMap::from([(
        "fyi.opensocial.rule/one".to_string(),
        atproto::drisl::encode(&atproto::Value::from_json(&rule).unwrap()),
    )]);
    assert_eq!(res.bytes().await.unwrap().to_vec(), star(&commit, &records));
}
