//! The host as the authority for its groups' spaces: who may read and
//! write each, syncs authorized for a delegation, the writer set it keeps
//! from what repo hosts tell it and from members leaving, and the syncers
//! it tells in turn, at the endpoints they registered; and as a repo host,
//! word of a write owed to an authority elsewhere until it lands.

mod common;

use atproto::service_auth::{self, Claims};
use atproto::space::COMMIT_MAC;
use atproto::space_auth::{self, Kind, Make, sign_request};
use atproto::{Curve, Keypair, Value};
use common::spaces::*;
use common::{Running, start};
use group_host::authority::{Access, Delivery, retry_delay, syncer_retry_delay};
use group_host::store::{Notice, Registration, SyncerNotice, SyncerUpdate};
use serde_json::{Value as Json, json};
use std::sync::atomic::Ordering;

const KIND: &str = "org.atgrp.content";
const AUTHORIZE: &str = "com.atproto.space.authorizeSpaceSync";
const REPO_WRITE: &str = "com.atproto.space.defs#repoWrite";
const REPO_REMOVED: &str = "com.atproto.space.defs#repoRemoved";
const EXPIRED: &str = "com.atproto.space.notifySpaceUpdate#registrationExpired";
const DELETED: &str = "com.atproto.space.notifySpaceUpdate#spaceDeleted";

/// A group here and a space of it, and two accounts elsewhere.
struct Here {
    h: Running,
    group: String,
    space: String,
    alice: Authority,
    bob: Authority,
}

/// A host on http, so that it calls syncers' endpoints here on http too.
async fn here() -> Here {
    let h = start(&[("PUBLIC_URL", "http://host.test")], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    let space = h
        .host
        .create_space(&group, KIND, "self", &closed())
        .unwrap();
    let alice = authority(&h, false, true).await;
    let bob = authority(&h, false, true).await;
    Here {
        h,
        group,
        space,
        alice,
        bob,
    }
}

/// A delegation token from `who`, for `space` of `authority`.
fn delegation(who: &Authority, space: &str, authority: &str) -> String {
    let aud = format!("{authority}#atproto_pds");
    let make = Make {
        iss: &who.did,
        sub: space,
        aud: Some(&aud),
        ..Make::default()
    };
    space_auth::create(Kind::Delegation, &make, &who.key, now()).unwrap()
}

/// `POST nsid` with `body` and `headers`: status and JSON.
async fn post(h: &Running, nsid: &str, body: &Json, headers: &[(&str, String)]) -> (u16, Json) {
    let mut req = h.http.post(format!("{}/xrpc/{nsid}", h.url)).json(body);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let res = req.send().await.unwrap();
    (
        res.status().as_u16(),
        res.json().await.unwrap_or(Json::Null),
    )
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// A sync authorized for `body`.
async fn authorize(h: &Running, body: Json) -> (u16, Json) {
    post(h, AUTHORIZE, &body, &[]).await
}

/// What a syncer asks with: a key of its own, `who`'s delegation if
/// any, and the endpoint and token to be told at if any.
fn asking(x: &Here, who: Option<&Authority>, notify: Option<(&str, &str)>) -> Json {
    let key = Keypair::generate(Curve::P256).public().did_key();
    let mut body = json!({"space": x.space, "credentialKey": key});
    if let Some(who) = who {
        body["delegationToken"] = json!(delegation(who, &x.space, &x.group));
    }
    if let Some((endpoint, token)) = notify {
        body["notify"] = json!({"endpoint": endpoint, "token": token});
    }
    body
}

fn at(endpoint: &str, token: &str) -> Registration {
    Registration {
        endpoint: endpoint.into(),
        token: token.into(),
    }
}

/// The word owed to syncers, once there is some.
async fn owed(h: &Running) -> Vec<SyncerNotice> {
    for _ in 0..200 {
        let owed = h.host.store.syncer_notices_due(i64::MAX, 10).unwrap();
        if !owed.is_empty() {
            return owed;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("nothing owed");
}

fn owed_now(h: &Running) -> Vec<SyncerNotice> {
    h.host.store.syncer_notices_due(i64::MAX, 10).unwrap()
}

/// Makes each word owed due now.
fn due_now(h: &Running) {
    for notice in owed_now(h) {
        h.host.store.retry_syncer(&notice, 0).unwrap();
    }
}

/// A change to the group's space: its rule at `rkey`, written.
async fn write(x: &Here, rkey: &str) {
    let rule = [create(RULE, rkey, rule("A"))];
    x.h.host
        .space_write(&x.group, &x.space, &rule)
        .await
        .unwrap();
}

/// Until when `to` is registered for `space`, once it is as `wanted` says.
async fn registered(h: &Running, space: &str, to: &Registration, wanted: bool) -> Option<i64> {
    for _ in 0..200 {
        let until = h.host.store.registration(space, to).unwrap();
        if until.is_some() == wanted {
            return until;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("not {wanted} as registered");
}

#[tokio::test]
async fn makes_spaces_and_grants() {
    let x = here().await;
    let host = &x.h.host;
    let again = host
        .create_space(&x.group, KIND, "self", &closed())
        .unwrap_err();
    assert_eq!(again.error, "SpaceAlreadyExists");
    let elsewhere = host
        .create_space(&x.alice.did, KIND, "self", &closed())
        .unwrap_err();
    assert_eq!(
        elsewhere.message,
        format!("{} is no group here", x.alice.did)
    );
    let bad = host
        .create_space(&x.group, "content", "self", &closed())
        .unwrap_err();
    assert_eq!(bad.error, "InvalidRequest");
    assert_eq!(
        host.grant("at:space/did:plc:none/a.b.c/self", &x.alice.did, true, true)
            .unwrap_err()
            .error,
        "SpaceNotFound"
    );
    assert_eq!(
        host.grant(&x.space, "alice", true, true)
            .unwrap_err()
            .message,
        "alice is no DID"
    );
    assert_eq!(
        host.ungrant("at:space/did:plc:none/a.b.c/self", &x.alice.did)
            .unwrap_err()
            .error,
        "SpaceNotFound"
    );

    // The group may do anything in its own space; others what they are
    // granted; with none, read only a space open to all.
    let space = host.store.space(&x.space).unwrap().unwrap();
    let may = |user: Option<&str>, access| host.may(&x.space, &space, user, access).unwrap();
    assert!(may(Some(&x.group), Access::Write));
    assert!(!may(Some(&x.alice.did), Access::Read));
    assert!(!may(None, Access::Read));
    host.grant(&x.space, &x.alice.did, true, false).unwrap();
    let space = host.store.space(&x.space).unwrap().unwrap();
    let may = |user: Option<&str>, access| host.may(&x.space, &space, user, access).unwrap();
    assert!(may(Some(&x.alice.did), Access::Read));
    assert!(!may(Some(&x.alice.did), Access::Write));
    host.ungrant(&x.space, &x.alice.did).unwrap();
    assert!(
        !host
            .may(&x.space, &space, Some(&x.alice.did), Access::Read)
            .unwrap()
    );
    let open = host.create_space(&x.group, KIND, "open", &open()).unwrap();
    let opened = host.store.space(&open).unwrap().unwrap();
    assert!(host.may(&open, &opened, None, Access::Read).unwrap());
    assert!(!host.may(&open, &opened, None, Access::Write).unwrap());
    assert!(
        host.may(&open, &opened, Some(&x.bob.did), Access::Read)
            .unwrap()
    );
}

#[tokio::test]
async fn authorizes_a_user_the_space_lets_read() {
    let x = here().await;
    x.h.host.grant(&x.space, &x.alice.did, true, false).unwrap();
    // Before the space's first change, no revision.
    let (status, first) = authorize(&x.h, asking(&x, Some(&x.alice), None)).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first.get("spaceRev"), None);
    // The group's repo in its own space, for Alice to read.
    x.h.host
        .space_write(&x.group, &x.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    let key = Keypair::generate(Curve::P256);
    let token = delegation(&x.alice, &x.space, &x.group);
    let body = json!({"space": x.space, "credentialKey": key.public().did_key(), "delegationToken": token, "clientAttestation": "not.checked.here"});
    let (status, issued) = authorize(&x.h, body.clone()).await;
    assert_eq!(status, 200, "{issued}");
    let credential = issued["credential"].as_str().unwrap();
    let (_, claims) = space_auth::parse(Kind::Credential, credential).unwrap();
    assert_eq!(claims.iss, x.group);
    assert_eq!(claims.sub, x.space);
    assert_eq!(claims.cnf.as_deref(), Some(key.public().did_key().as_str()));
    // With the space's revision now.
    let space_rev = x.h.host.store.space_rev(&x.space).unwrap().unwrap();
    assert_eq!(issued["spaceRev"], space_rev);

    // It reads the group's repo there.
    let kem =
        x.h.host
            .store
            .account(&x.group)
            .unwrap()
            .unwrap()
            .kem
            .public()
            .did_key();
    let s = Syncer {
        key: key.clone(),
        credential: credential.into(),
    };
    let (headers, _) = signed(&s, &x.group, &kem, COMMIT_MAC);
    let query = format!("space={}&repo={}", x.space, x.group);
    let got = read(&x.h, "com.atproto.space.getLatestCommit", &query, &headers).await;
    assert_eq!(got.status, 200);

    // One token buys one credential.
    let again = authorize(&x.h, body).await;
    assert_eq!(error(&again), (400, "InvalidDelegationToken"));
}

#[tokio::test]
async fn refuses_a_sync_it_should_not_authorize() {
    let x = here().await;
    let host = &x.h.host;
    host.grant(&x.space, &x.alice.did, true, false).unwrap();
    let kid = Keypair::generate(Curve::P256).public().did_key();
    let token = |who: &Authority, space: &str, authority: &str| delegation(who, space, authority);
    let elsewhere = format!("at:space/{}/{KIND}/self", x.alice.did);
    let never_made = format!("at:space/{}/{KIND}/never", x.group);
    let pretending = |did: &str| Authority {
        did: did.into(),
        key: Keypair::generate(Curve::K256),
        told: Default::default(),
        rotation: Keypair::generate(Curve::K256),
        pds: String::new(),
        last: None,
    };
    let stranger = pretending(&x.alice.did);
    let unknown = pretending("did:plc:unknown");
    let notify = |notify: Json| json!({"space": x.space, "credentialKey": kid, "notify": notify});
    for (body, refused) in [
        (json!({"credentialKey": kid}), (400, "InvalidRequest")),
        (
            json!({"space": "nope", "credentialKey": kid}),
            (400, "InvalidRequest"),
        ),
        (json!({"space": x.space}), (400, "InvalidRequest")),
        (
            json!({"space": x.space, "credentialKey": Keypair::generate(Curve::K256).public().did_key()}),
            (400, "InvalidRequest"),
        ),
        // An endpoint that is no http URL, or a token too long.
        (
            notify(json!({"endpoint": "not a url", "token": "t"})),
            (400, "InvalidRequest"),
        ),
        (
            notify(json!({"endpoint": "ftp://syncer.test", "token": "t"})),
            (400, "InvalidRequest"),
        ),
        (
            notify(json!({"endpoint": "https://syncer.test", "token": "t".repeat(513)})),
            (400, "InvalidRequest"),
        ),
        (
            notify(json!({"endpoint": "https://syncer.test"})),
            (400, "InvalidRequest"),
        ),
        (
            notify(json!("https://syncer.test")),
            (400, "InvalidRequest"),
        ),
        // No token: only a space anyone may read.
        (
            json!({"space": x.space, "credentialKey": kid}),
            (400, "SpaceNotFound"),
        ),
        // A user the space does not let read is told there is no space.
        (
            json!({"space": x.space, "credentialKey": kid, "delegationToken": token(&x.bob, &x.space, &x.group)}),
            (400, "SpaceNotFound"),
        ),
        // A space whose authority is not here, or never made.
        (
            json!({"space": elsewhere, "credentialKey": kid, "delegationToken": token(&x.alice, &elsewhere, &x.alice.did)}),
            (400, "SpaceNotFound"),
        ),
        (
            json!({"space": never_made, "credentialKey": kid, "delegationToken": token(&x.alice, &never_made, &x.group)}),
            (400, "SpaceNotFound"),
        ),
        (
            json!({"space": x.space, "credentialKey": kid, "delegationToken": "x.y.z"}),
            (400, "InvalidDelegationToken"),
        ),
        // For another authority, another space, signed by another key, or
        // by someone with no key to be found.
        (
            json!({"space": x.space, "credentialKey": kid, "delegationToken": token(&x.alice, &x.space, &x.bob.did)}),
            (400, "InvalidDelegationToken"),
        ),
        (
            json!({"space": x.space, "credentialKey": kid, "delegationToken": token(&x.alice, &elsewhere, &x.group)}),
            (400, "InvalidDelegationToken"),
        ),
        // For another space of the same authority.
        (
            json!({"space": x.space, "credentialKey": kid, "delegationToken": token(&x.alice, &never_made, &x.group)}),
            (400, "InvalidDelegationToken"),
        ),
        (
            json!({"space": x.space, "credentialKey": kid, "delegationToken": token(&stranger, &x.space, &x.group)}),
            (400, "InvalidDelegationToken"),
        ),
        (
            json!({"space": x.space, "credentialKey": kid, "delegationToken": token(&unknown, &x.space, &x.group)}),
            (400, "InvalidDelegationToken"),
        ),
    ] {
        let said = authorize(&x.h, body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }

    // A space open to all authorizes anyone, with no token.
    let open = host.create_space(&x.group, KIND, "open", &open()).unwrap();
    let (status, _) = authorize(&x.h, json!({"space": open, "credentialKey": kid})).await;
    assert_eq!(status, 200);

    // Its authority switched off, nobody.
    host.deactivate_account(&x.group, &x.group, &[]).unwrap();
    let said = authorize(&x.h, json!({"space": open, "credentialKey": kid})).await;
    assert_eq!(error(&said), (400, "RepoDeactivated"));
    host.activate_account(&x.group).unwrap();

    // A host on https calls endpoints on https alone.
    let h = start(&[], false).await;
    let body = json!({"space": open, "credentialKey": kid, "notify": {"endpoint": "http://syncer.test", "token": "t"}});
    let said = authorize(&h, body).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

#[tokio::test]
async fn a_space_deleted_says_so_until_it_is_made_again() {
    let x = here().await;
    let host = &x.h.host;
    host.end_space(&x.group, &x.space).unwrap();
    // To anyone, before any token is looked at.
    let said = authorize(&x.h, asking(&x, None, None)).await;
    assert_eq!(error(&said), (400, "SpaceDeleted"));
    let said = authorize(&x.h, asking(&x, Some(&x.bob), None)).await;
    assert_eq!(error(&said), (400, "SpaceDeleted"));
    assert!(host.store.space_deleted(&x.space).unwrap());

    host.create_space(&x.group, KIND, "self", &open()).unwrap();
    assert!(!host.store.space_deleted(&x.space).unwrap());
    let (status, _) = authorize(&x.h, asking(&x, None, None)).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn registers_a_syncer_once_its_endpoint_confirms() {
    let x = here().await;
    let host = &x.h.host;
    host.grant(&x.space, &x.alice.did, true, false).unwrap();
    let (url, e) = endpoint().await;
    let (status, issued) = authorize(&x.h, asking(&x, Some(&x.alice), Some((&url, "tok")))).await;
    assert_eq!(status, 200, "{issued}");
    // It was asked, with a challenge it echoed.
    let asked = e.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["space"], x.space);
    assert_eq!(asked[0]["token"], "tok");
    assert!(asked[0]["challenge"].as_str().unwrap().len() >= 16);
    // Registered for as long as the credential lasts.
    let (_, claims) =
        space_auth::parse(Kind::Credential, issued["credential"].as_str().unwrap()).unwrap();
    let to = at(&url, "tok");
    assert_eq!(
        host.store.registration(&x.space, &to).unwrap(),
        Some(claims.exp as i64)
    );

    // Lapsed, it is renewed by the next authorization, unasked.
    host.store
        .register(&x.space, &to, now() as i64 - 1)
        .unwrap();
    let said = authorize(&x.h, asking(&x, Some(&x.alice), Some((&url, "tok")))).await;
    assert_eq!(said.0, 200, "{}", said.1);
    assert_eq!(e.asked.lock().unwrap().len(), 1);
    assert!(host.store.registration(&x.space, &to).unwrap().unwrap() > now() as i64);

    // An endpoint that echoes another challenge, or answers nothing, is
    // not registered.
    e.wrong.store(true, Ordering::SeqCst);
    let said = authorize(&x.h, asking(&x, Some(&x.alice), Some((&url, "other")))).await;
    assert_eq!(error(&said), (400, "RegistrationNotConfirmed"));
    assert_eq!(
        host.store
            .registration(&x.space, &at(&url, "other"))
            .unwrap(),
        None
    );
    let nowhere = "http://127.0.0.1:1";
    let said = authorize(&x.h, asking(&x, Some(&x.alice), Some((nowhere, "tok")))).await;
    assert_eq!(error(&said), (400, "RegistrationNotConfirmed"));

    // One the space lets read no more is refused, and its registration
    // withdrawn.
    let said = authorize(&x.h, asking(&x, Some(&x.bob), Some((&url, "tok")))).await;
    assert_eq!(error(&said), (400, "SpaceNotFound"));
    assert_eq!(host.store.registration(&x.space, &to).unwrap(), None);
}

/// `notifyRepoUpdate` for `repo` at `rev`, with `data`, signed by `who`'s
/// key as `iss` for `aud`.
async fn notify(x: &Here, who: &Keypair, iss: &str, aud: &str, body: &Json) -> (u16, Json) {
    let claims = Claims {
        iss: iss.into(),
        aud: aud.into(),
        exp: now() + 60,
        iat: Some(now()),
        lxm: Some("com.atproto.space.notifyRepoUpdate".into()),
        jti: None,
    };
    let token = service_auth::sign(who, &claims);
    let auth = [("authorization", format!("Bearer {token}"))];
    post(&x.h, "com.atproto.space.notifyRepoUpdate", body, &auth).await
}

fn rev(seconds_ahead: u64) -> String {
    atproto::tid::tid_from((now() + seconds_ahead) * 1_000_000, 1)
}

fn wrote(space: &str, repo: &str, rev: &str, data: &[u8]) -> Json {
    json!({"space": space, "repo": repo, "repoRev": rev, "data": Value::Bytes(data.to_vec()).to_json()})
}

/// The authority's credential, for `listWriters`: Alice's, whom the space
/// lets read.
async fn authority_headers(x: &Here) -> Vec<(&'static str, String)> {
    let key = Keypair::generate(Curve::P256);
    let token = delegation(&x.alice, &x.space, &x.group);
    let body = json!({"space": x.space, "credentialKey": key.public().did_key(), "delegationToken": token});
    let (_, issued) = authorize(&x.h, body).await;
    let authorization = format!(
        "Atproto-Space-Credential {}",
        issued["credential"].as_str().unwrap()
    );
    sign_request(&key, &authorization, &x.group)
        .unwrap()
        .into_iter()
        .collect()
}

async fn list_writers(x: &Here, rest: &str, headers: &[(&str, String)]) -> (u16, Json) {
    let got = read(
        &x.h,
        "com.atproto.space.listWriters",
        &format!("space={}{rest}", x.space),
        headers,
    )
    .await;
    (got.status, serde_json::from_slice(&got.body).unwrap())
}

#[tokio::test]
async fn keeps_the_writers_repo_hosts_tell_it_of() {
    let x = here().await;
    let host = &x.h.host;
    host.grant(&x.space, &x.alice.did, true, true).unwrap();
    host.grant(&x.space, &x.bob.did, true, false).unwrap();
    let aud = format!("{}#atproto_pds", x.group);
    let first = rev(0);
    let body = wrote(&x.space, &x.alice.did, &first, &[1; 32]);
    let told = notify(&x, &x.alice.key, &x.alice.did, &aud, &body).await;
    assert_eq!(told, (200, json!({})));
    // The group writes there too, kept as it writes.
    let commit = host
        .space_write(&x.group, &x.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    // A notice of a revision known already, or an earlier one, changes nothing.
    let older = wrote(&x.space, &x.alice.did, "2222222222222", &[2; 32]);
    assert_eq!(
        notify(&x, &x.alice.key, &x.alice.did, &x.group, &older)
            .await
            .0,
        200
    );

    let headers = authority_headers(&x).await;
    let (status, all) = list_writers(&x, "", &headers).await;
    assert_eq!(status, 200, "{all}");
    let repos = all["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0]["$type"], REPO_WRITE);
    assert_eq!(repos[0]["repo"], x.alice.did);
    assert_eq!(repos[0]["repoRev"], first);
    assert_eq!(repos[0]["data"], Value::Bytes(vec![1; 32]).to_json());
    assert_eq!(repos[0].get("prevSpaceRev"), None);
    assert_eq!(repos[1]["repo"], x.group);
    assert_eq!(repos[1]["repoRev"], commit.rev);
    assert_eq!(repos[1]["prevSpaceRev"], repos[0]["spaceRev"]);
    assert!(repos[0]["spaceRev"].as_str() < repos[1]["spaceRev"].as_str());
    // A cursor after any page with something in it: the last one's.
    assert_eq!(all["cursor"], repos[1]["spaceRev"]);
    let (_, page) = list_writers(&x, "&limit=1", &headers).await;
    let cursor = page["cursor"].as_str().unwrap().to_string();
    assert_eq!(cursor, repos[0]["spaceRev"]);
    let (_, rest) = list_writers(&x, &format!("&cursor={cursor}"), &headers).await;
    assert_eq!(rest["repos"].as_array().unwrap().len(), 1);
    let (_, none) = list_writers(
        &x,
        &format!("&cursor={}", repos[1]["spaceRev"].as_str().unwrap()),
        &headers,
    )
    .await;
    assert_eq!(none, json!({"repos": []}));
    // Only for a credential to the space.
    let (status, _) = list_writers(&x, "", &[]).await;
    assert_eq!(status, 401);

    // Alice taken out of the set: listed as removed, after the rest.
    host.remove_writer(&x.space, &x.alice.did).unwrap();
    let (_, all) = list_writers(&x, "", &headers).await;
    let repos = all["repos"].as_array().unwrap().clone();
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0]["repo"], x.group);
    assert_eq!(
        repos[1],
        json!({"$type": REPO_REMOVED, "repo": x.alice.did, "spaceRev": repos[1]["spaceRev"], "prevSpaceRev": repos[0]["spaceRev"]})
    );
    assert_eq!(
        host.store.space_rev(&x.space).unwrap().as_deref(),
        repos[1]["spaceRev"].as_str()
    );
    // Live writers are the group alone.
    let live = host.store.writers(&x.space, None, 10).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].did, x.group);
    // A newer write of hers puts her back.
    let later = wrote(&x.space, &x.alice.did, &rev(1), &[5; 32]);
    let told = notify(&x, &x.alice.key, &x.alice.did, &aud, &later).await;
    assert_eq!(told, (200, json!({})));
    let (_, all) = list_writers(&x, "", &headers).await;
    let back = all["repos"].as_array().unwrap()[1].clone();
    assert_eq!(back["$type"], REPO_WRITE);
    assert_eq!(back["repo"], x.alice.did);
    assert_eq!(host.store.writers(&x.space, None, 10).unwrap().len(), 2);

    let refusals = [
        // Of another account than the caller.
        (
            notify(&x, &x.bob.key, &x.bob.did, &aud, &body).await,
            (403, "Forbidden"),
        ),
        // For another authority.
        (
            notify(&x, &x.alice.key, &x.alice.did, &x.bob.did, &body).await,
            (403, "Forbidden"),
        ),
        // By one the space does not let write.
        (
            notify(
                &x,
                &x.bob.key,
                &x.bob.did,
                &aud,
                &wrote(&x.space, &x.bob.did, &rev(0), &[3; 32]),
            )
            .await,
            (403, "Forbidden"),
        ),
        (
            notify(
                &x,
                &x.alice.key,
                &x.alice.did,
                &aud,
                &wrote(&x.space, &x.alice.did, &rev(3600), &[3; 32]),
            )
            .await,
            (400, "FutureRev"),
        ),
        (
            notify(
                &x,
                &x.alice.key,
                &x.alice.did,
                &aud,
                &wrote(&x.space, &x.alice.did, "nope", &[3; 32]),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            notify(
                &x,
                &x.alice.key,
                &x.alice.did,
                &aud,
                &json!({"space": x.space, "repo": x.alice.did, "repoRev": rev(0), "data": "x"}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            notify(
                &x,
                &x.alice.key,
                &x.alice.did,
                &aud,
                &json!({"space": "nope"}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            notify(
                &x,
                &x.alice.key,
                &x.alice.did,
                &x.alice.did,
                &wrote(
                    &format!("at:space/{}/{KIND}/self", x.alice.did),
                    &x.alice.did,
                    &rev(0),
                    &[3; 32],
                ),
            )
            .await,
            (400, "SpaceNotFound"),
        ),
        (
            notify(
                &x,
                &x.alice.key,
                &x.alice.did,
                &aud,
                &json!({"space": x.space}),
            )
            .await,
            (400, "InvalidRequest"),
        ),
    ];
    for (said, refused) in refusals {
        assert_eq!(error(&said), refused, "{}", said.1);
    }
}

#[tokio::test]
async fn tells_the_syncers_registered_of_each_change() {
    let x = here().await;
    let host = &x.h.host;
    host.grant(&x.space, &x.alice.did, true, true).unwrap();
    let (url, e) = endpoint().await;
    let (status, _) = authorize(&x.h, asking(&x, Some(&x.alice), Some((&url, "tok")))).await;
    assert_eq!(status, 200);

    let first = host
        .space_write(&x.group, &x.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    e.told(1).await;
    let aud = format!("{}#atproto_pds", x.group);
    let second = rev(0);
    let body = wrote(&x.space, &x.alice.did, &second, &[4; 32]);
    notify(&x, &x.alice.key, &x.alice.did, &aud, &body).await;
    let told = e.told(2).await;
    assert_eq!(told[0]["space"], x.space);
    assert_eq!(told[0]["token"], "tok");
    let update = &told[0]["update"];
    assert_eq!(update["$type"], REPO_WRITE);
    assert_eq!(update["repo"], x.group);
    assert_eq!(update["repoRev"], first.rev);
    assert_eq!(update["data"], Value::Bytes(first.data.to_vec()).to_json());
    assert_eq!(update.get("prevSpaceRev"), None);
    let update = &told[1]["update"];
    assert_eq!(update["repo"], x.alice.did);
    assert_eq!(update["repoRev"], second);
    assert_eq!(update["prevSpaceRev"], told[0]["update"]["spaceRev"]);

    // A writer taken out of the set: told so.
    host.remove_writer(&x.space, &x.alice.did).unwrap();
    let told = e.told(3).await;
    assert_eq!(
        told[2]["update"],
        json!({"$type": REPO_REMOVED, "repo": x.alice.did, "spaceRev": told[2]["update"]["spaceRev"], "prevSpaceRev": told[1]["update"]["spaceRev"]})
    );
    // One not in it: nothing to tell.
    host.remove_writer(&x.space, &x.alice.did).unwrap();
    assert_eq!(e.told_by_now().await.len(), 3);

    // One that refuses holds nothing up, and is owed nothing.
    e.answer(500);
    host.space_write(&x.group, &x.space, &[create(RULE, "c", rule("C"))])
        .await
        .unwrap();
    e.told(4).await;
    assert_eq!(e.told_by_now().await.len(), 4);
    assert_eq!(owed_now(&x.h), vec![]);
}

#[tokio::test]
async fn tells_a_syncer_once_its_registration_lapsed() {
    let x = here().await;
    let host = &x.h.host;
    let (url, e) = endpoint().await;
    let to = at(&url, "tok");
    host.store
        .register(&x.space, &to, now() as i64 - 1)
        .unwrap();
    // The first change after: word of the lapse, in its place.
    host.space_write(&x.group, &x.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    let space_rev = host.store.space_rev(&x.space).unwrap().unwrap();
    let told = e.told(1).await;
    assert_eq!(
        told[0],
        json!({"space": x.space, "token": "tok", "update": {"$type": EXPIRED, "spaceRev": space_rev}})
    );
    // Once: the next change tells it nothing.
    host.space_write(&x.group, &x.space, &[create(RULE, "b", rule("B"))])
        .await
        .unwrap();
    assert_eq!(e.told_by_now().await.len(), 1);
    assert_eq!(owed_now(&x.h), vec![]);
    // Still registered, lapsed, to renew without confirming.
    assert!(host.store.registration(&x.space, &to).unwrap().is_some());
}

#[test]
fn waits_longer_between_tries_of_a_syncer() {
    assert_eq!(syncer_retry_delay(0), 60);
    assert_eq!(syncer_retry_delay(1), 60);
    assert_eq!(syncer_retry_delay(2), 600);
    assert_eq!(syncer_retry_delay(3), 3600);
    assert_eq!(syncer_retry_delay(4), 6 * 3600);
    assert_eq!(syncer_retry_delay(5), 12 * 3600);
    assert_eq!(syncer_retry_delay(9), 12 * 3600);
}

#[tokio::test]
async fn owes_a_syncer_word_of_its_lapse_until_it_lands() {
    let x = here().await;
    let host = &x.h.host;
    let (url, e) = endpoint().await;
    let to = at(&url, "tok");
    host.store
        .register(&x.space, &to, now() as i64 - 1)
        .unwrap();
    e.answer(500);
    host.space_write(&x.group, &x.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    let space_rev = host.store.space_rev(&x.space).unwrap().unwrap();
    let owed = owed(&x.h).await;
    let at_now = now() as i64;
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].to, to);
    assert_eq!(
        owed[0].update,
        SyncerUpdate::RegistrationExpired {
            space_rev: space_rev.clone()
        }
    );
    assert_eq!(owed[0].attempts, 1);
    assert!((at_now + 58..=at_now + 60).contains(&owed[0].next_at));
    let three_days = 72 * 60 * 60;
    assert!((at_now + three_days - 2..=at_now + three_days).contains(&owed[0].deliver_by));
    // Not due yet.
    assert_eq!(host.retry_syncers_due().await.unwrap(), 0);

    // Due, and refused again: tried later, after a longer wait.
    due_now(&x.h);
    assert_eq!(host.retry_syncers_due().await.unwrap(), 1);
    let again = owed_now(&x.h);
    assert_eq!(again[0].attempts, 3);
    assert!(again[0].next_at >= now() as i64 + 3600 - 2);
    assert_eq!(e.told.lock().unwrap().len(), 2);

    // Its authority switched off: kept, and not tried.
    host.deactivate_account(&x.group, &x.group, &[]).unwrap();
    due_now(&x.h);
    assert_eq!(host.retry_syncers_due().await.unwrap(), 1);
    assert_eq!(e.told.lock().unwrap().len(), 2);
    assert_eq!(owed_now(&x.h)[0].attempts, 5);
    host.activate_account(&x.group).unwrap();

    // Heard at last: owed no more, and still registered, lapsed.
    e.answer(200);
    due_now(&x.h);
    assert_eq!(host.retry_syncers_due().await.unwrap(), 1);
    let told = e.told.lock().unwrap().clone();
    assert_eq!(told.len(), 3);
    assert_eq!(
        told[2]["update"],
        json!({"$type": EXPIRED, "spaceRev": space_rev})
    );
    assert_eq!(owed_now(&x.h), vec![]);
    assert!(host.store.registration(&x.space, &to).unwrap().is_some());

    // Renewed since it was owed: dropped, unsent.
    let notice = SyncerNotice {
        space: x.space.clone(),
        to: to.clone(),
        update: SyncerUpdate::RegistrationExpired {
            space_rev: space_rev.clone(),
        },
        attempts: 1,
        next_at: 0,
        deliver_by: now() as i64 + 60,
    };
    host.store.owe_syncer(&notice).unwrap();
    host.store
        .register(&x.space, &to, now() as i64 + 60)
        .unwrap();
    assert_eq!(host.retry_syncers_due().await.unwrap(), 1);
    assert_eq!(e.told.lock().unwrap().len(), 3);
    assert_eq!(owed_now(&x.h), vec![]);
}

#[tokio::test]
async fn a_syncer_never_told_of_its_lapse_is_registered_no_more() {
    let x = here().await;
    let host = &x.h.host;
    let (url, e) = endpoint().await;
    let to = at(&url, "tok");
    let lapse = || {
        host.store
            .register(&x.space, &to, now() as i64 - 1)
            .unwrap()
    };

    // Refused for good at once.
    lapse();
    e.answer(400);
    write(&x, "a").await;
    e.told(1).await;
    registered(&x.h, &x.space, &to, false).await;
    assert_eq!(owed_now(&x.h), vec![]);

    // Refused for good later.
    lapse();
    e.answer(500);
    write(&x, "b").await;
    owed(&x.h).await;
    e.answer(400);
    due_now(&x.h);
    assert_eq!(host.retry_syncers_due().await.unwrap(), 1);
    assert_eq!(host.store.registration(&x.space, &to).unwrap(), None);
    assert_eq!(owed_now(&x.h), vec![]);

    // Not reached in three days.
    lapse();
    e.answer(500);
    let notice = SyncerNotice {
        space: x.space.clone(),
        to: to.clone(),
        update: SyncerUpdate::RegistrationExpired {
            space_rev: "3l5mz5mkbn22b".into(),
        },
        attempts: 9,
        next_at: 0,
        deliver_by: 0,
    };
    host.store.owe_syncer(&notice).unwrap();
    assert_eq!(host.retry_syncers_due().await.unwrap(), 1);
    assert_eq!(host.store.registration(&x.space, &to).unwrap(), None);
    assert_eq!(owed_now(&x.h), vec![]);
}

#[tokio::test]
async fn tells_every_syncer_of_a_space_deleted_until_it_lands() {
    let x = here().await;
    let host = &x.h.host;
    let (url, e) = endpoint().await;
    let (active, lapsed) = (at(&url, "a"), at(&url, "b"));
    // One whose name is never found, and one that takes no connection.
    let nowhere = at("http://nowhere.invalid", "c");
    let closed = at("http://127.0.0.1:1", "d");
    // And one that refuses word of it for good, owed nothing after.
    let (refusing_url, refusing) = endpoint().await;
    refusing.answer(400);
    let refused = at(&refusing_url, "e");
    let until = now() as i64 + 60;
    host.store.register(&x.space, &active, until).unwrap();
    host.store
        .register(&x.space, &lapsed, now() as i64 - 1)
        .unwrap();
    host.store.register(&x.space, &nowhere, until).unwrap();
    host.store.register(&x.space, &closed, until).unwrap();
    host.store.register(&x.space, &refused, until).unwrap();
    // Word of its lapse owed to one, not yet due.
    let expired = |to: &Registration| SyncerNotice {
        space: x.space.clone(),
        to: to.clone(),
        update: SyncerUpdate::RegistrationExpired {
            space_rev: "3l5mz5mkbn22b".into(),
        },
        attempts: 1,
        next_at: i64::MAX - 1,
        deliver_by: i64::MAX,
    };
    host.store.owe_syncer(&expired(&lapsed)).unwrap();

    e.answer(500);
    host.end_space(&x.group, &x.space).unwrap();
    let told = e.told_of(DELETED, 2).await;
    let gone = json!({"$type": DELETED});
    let mut tokens: Vec<&str> = told
        .iter()
        .map(|t| {
            assert_eq!(t["space"], x.space);
            assert_eq!(t["update"], gone);
            t["token"].as_str().unwrap()
        })
        .collect();
    tokens.sort();
    assert_eq!(tokens, ["a", "b"]);
    refusing.told_of(DELETED, 1).await;
    // Each owed it, the lapse's word replaced; and none registered now.
    for _ in 0..200 {
        if owed_now(&x.h).len() == 4 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let owed = owed_now(&x.h);
    assert_eq!(owed.len(), 4);
    assert!(owed.iter().all(|n| n.update == SyncerUpdate::SpaceDeleted));
    for to in [&active, &lapsed, &nowhere, &closed, &refused] {
        assert_eq!(host.store.registration(&x.space, to).unwrap(), None);
    }
    // Word of a lapse never replaces a deletion's.
    host.store.owe_syncer(&expired(&active)).unwrap();
    let owed = owed_now(&x.h);
    assert!(owed.iter().all(|n| n.update == SyncerUpdate::SpaceDeleted));

    // Heard where it is heard; tried again where it is not.
    e.answer(200);
    due_now(&x.h);
    assert_eq!(host.retry_syncers_due().await.unwrap(), 4);
    assert_eq!(e.told.lock().unwrap().len(), 4);
    let mut owed: Vec<(String, i64)> = owed_now(&x.h)
        .into_iter()
        .map(|n| (n.to.token, n.attempts))
        .collect();
    owed.sort();
    assert_eq!(owed, [("c".to_string(), 3), ("d".to_string(), 3)]);

    // The space made again: what is owed is so no more.
    host.create_space(&x.group, KIND, "self", &common::spaces::closed())
        .unwrap();
    due_now(&x.h);
    assert_eq!(host.retry_syncers_due().await.unwrap(), 2);
    assert_eq!(owed_now(&x.h), vec![]);
}

#[tokio::test]
async fn a_group_writing_where_it_may_not_is_left_out() {
    let x = here().await;
    let other = x.h.host.create_account("hikers.groups.test").await.unwrap();
    let theirs =
        x.h.host
            .create_space(&other, KIND, "self", &closed())
            .unwrap();
    // The write is made in the group's repo; the space does not count it.
    x.h.host
        .space_write(&x.group, &theirs, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    assert_eq!(x.h.host.store.writers(&theirs, None, 10).unwrap(), vec![]);
}

#[test]
fn waits_longer_between_tries_up_to_an_hour() {
    assert_eq!(retry_delay(1), 10);
    assert_eq!(retry_delay(2), 20);
    assert_eq!(retry_delay(4), 80);
    assert_eq!(retry_delay(30), 3600);
    assert_eq!(retry_delay(0), 10);
}

#[tokio::test]
async fn owes_an_authority_word_of_a_write_until_it_lands() {
    // On http, as on a network of test servers: the PDS is on this machine.
    let h = start(&[("PUBLIC_URL", "http://host.test")], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    let elsewhere = authority(&h, true, true).await;
    let space = format!("at:space/{}/{KIND}/self", elsewhere.did);
    let commit = h
        .host
        .space_write(&group, &space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    // Refused for now: owed, and tried again later, not yet.
    let later = now() as i64 + 20;
    let owed = h.host.store.due(later, 10).unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(
        (owed[0].rev.as_str(), owed[0].attempts),
        (commit.rev.as_str(), 1)
    );
    assert_eq!(h.host.deliver_due().await.unwrap(), 0);

    // Due, and heard this time, as notifyRepoUpdate from the group to the
    // authority's space host.
    elsewhere.told.answer(200);
    h.host.store.retry(&owed[0], 0).unwrap();
    assert_eq!(h.host.deliver_due().await.unwrap(), 1);
    let told = elsewhere.told.writes.lock().unwrap().clone();
    assert_eq!(told.len(), 2);
    assert_eq!(told[1].1["repoRev"], commit.rev);
    let token = told[1].0.strip_prefix("Bearer ").unwrap();
    let signing = h.host.store.account(&group).unwrap().unwrap().signing;
    let claims = service_auth::verify(token, &signing.public(), now()).unwrap();
    let aud = format!("{}#atproto_pds", elsewhere.did);
    let lxm = "com.atproto.space.notifyRepoUpdate";
    assert_eq!(service_auth::check(&claims, &aud, lxm).unwrap(), group);
    assert_eq!(h.host.store.due(later, 10).unwrap(), vec![]);

    // A refusal for good is given up.
    elsewhere.told.answer(400);
    h.host
        .space_write(&group, &space, &[create(RULE, "b", rule("B"))])
        .await
        .unwrap();
    assert_eq!(h.host.store.due(later, 10).unwrap(), vec![]);

    // A newer write's word takes the place of an older one's still owed.
    let notice = |rev: &str| Notice {
        repo: group.clone(),
        space: space.clone(),
        rev: rev.into(),
        data: vec![0; 32],
        attempts: 0,
    };
    let at = now() as i64;
    h.host
        .store
        .owe(&notice("2222222222222"), at, at + 60)
        .unwrap();
    h.host
        .store
        .owe(&notice("3333333333333"), at, at + 60)
        .unwrap();
    h.host.store.delivered(&notice("2222222222222")).unwrap();
    let owed = h.host.store.due(at, 10).unwrap();
    assert_eq!(owed[0].rev, "3333333333333");
    // And one a day old is given up.
    h.host.store.owe(&notice("4444444444444"), at, at).unwrap();
    assert_eq!(h.host.deliver_due().await.unwrap(), 0);
    assert_eq!(h.host.store.due(at, 10).unwrap(), vec![]);

    // What can never land is not tried again: a space that is not one, or a
    // repo not here.
    for bad in [
        Notice {
            space: "nope".into(),
            ..notice("5555555555555")
        },
        Notice {
            repo: "did:plc:nobody".into(),
            ..notice("5555555555555")
        },
    ] {
        assert_eq!(h.host.deliver(&bad).await, Delivery::Failed);
    }
}
