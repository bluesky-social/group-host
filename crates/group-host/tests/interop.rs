//! The host against the reference network (`harness/pds.mjs`): a real PLC
//! directory, and on the space alpha, a real member's PDS.

mod common;

use atproto::did;
use atproto::space_auth::{Expect, Kind, space_host_aud, verify};
use common::net::net;
use common::start;
use serde_json::{Value, json};

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
