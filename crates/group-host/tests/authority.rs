//! The host as the authority for its groups' spaces: who may read and
//! write each, credentials issued for a delegation, the writer set it
//! keeps from what repo hosts tell it, and the services it tells in turn;
//! and as a repo host, word of a write owed to an authority elsewhere
//! until it lands.

mod common;

use atproto::service_auth::{self, Claims};
use atproto::space::COMMIT_MAC;
use atproto::space_auth::{self, Kind, Make, sign_request};
use atproto::{Curve, Keypair, Value};
use common::spaces::*;
use common::{Running, start};
use group_host::authority::{Access, Delivery, retry_delay};
use group_host::store::Notice;
use serde_json::{Value as Json, json};

const KIND: &str = "fyi.opensocial.content";

/// A group here and a space of it, and two accounts elsewhere.
struct Here {
    h: Running,
    group: String,
    space: String,
    alice: Authority,
    bob: Authority,
}

async fn here() -> Here {
    let h = start(&[], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    let space = h.host.create_space(&group, KIND, "self", false).unwrap();
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

/// A credential issued for `body`.
async fn issue(h: &Running, body: Json) -> (u16, Json) {
    post(h, "com.atproto.space.getSpaceCredential", &body, &[]).await
}

#[tokio::test]
async fn makes_spaces_and_grants() {
    let x = here().await;
    let host = &x.h.host;
    let again = host
        .create_space(&x.group, KIND, "self", false)
        .unwrap_err();
    assert_eq!(again.error, "SpaceAlreadyExists");
    let elsewhere = host
        .create_space(&x.alice.did, KIND, "self", false)
        .unwrap_err();
    assert_eq!(
        elsewhere.message,
        format!("{} is no group here", x.alice.did)
    );
    let bad = host
        .create_space(&x.group, "content", "self", false)
        .unwrap_err();
    assert_eq!(bad.error, "InvalidRequest");
    assert_eq!(
        host.grant(
            "at://did:plc:none/space/a.b.c/self",
            &x.alice.did,
            true,
            true
        )
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
        host.ungrant("at://did:plc:none/space/a.b.c/self", &x.alice.did)
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
    let open = host.create_space(&x.group, KIND, "open", true).unwrap();
    let opened = host.store.space(&open).unwrap().unwrap();
    assert!(host.may(&open, &opened, None, Access::Read).unwrap());
    assert!(!host.may(&open, &opened, None, Access::Write).unwrap());
    assert!(
        host.may(&open, &opened, Some(&x.bob.did), Access::Read)
            .unwrap()
    );
}

#[tokio::test]
async fn issues_a_credential_to_a_user_the_space_lets_read() {
    let x = here().await;
    x.h.host.grant(&x.space, &x.alice.did, true, false).unwrap();
    // The group's repo in its own space, for Alice to read.
    x.h.host
        .space_write(&x.group, &x.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    let key = Keypair::generate(Curve::P256);
    let token = delegation(&x.alice, &x.space, &x.group);
    let body = json!({"space": x.space, "credentialKid": key.public().did_key(), "delegationToken": token});
    let (status, issued) = issue(&x.h, body.clone()).await;
    assert_eq!(status, 200, "{issued}");
    let credential = issued["credential"].as_str().unwrap();
    let (_, claims) = space_auth::parse(Kind::Credential, credential).unwrap();
    assert_eq!(claims.iss, x.group);
    assert_eq!(claims.sub, x.space);
    assert_eq!(claims.cnf.as_deref(), Some(key.public().did_key().as_str()));

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
    let again = issue(&x.h, body).await;
    assert_eq!(error(&again), (400, "InvalidDelegationToken"));
}

#[tokio::test]
async fn refuses_a_credential_it_should_not_issue() {
    let x = here().await;
    let host = &x.h.host;
    host.grant(&x.space, &x.alice.did, true, false).unwrap();
    let kid = Keypair::generate(Curve::P256).public().did_key();
    let token = |who: &Authority, space: &str, authority: &str| delegation(who, space, authority);
    let elsewhere = format!("at://{}/space/{KIND}/self", x.alice.did);
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
    for (body, refused) in [
        (json!({"credentialKid": kid}), (400, "InvalidRequest")),
        (
            json!({"space": "nope", "credentialKid": kid}),
            (400, "InvalidRequest"),
        ),
        (json!({"space": x.space}), (400, "InvalidRequest")),
        (
            json!({"space": x.space, "credentialKid": Keypair::generate(Curve::K256).public().did_key()}),
            (400, "InvalidRequest"),
        ),
        // No token: only a space anyone may read.
        (
            json!({"space": x.space, "credentialKid": kid}),
            (400, "SpaceNotFound"),
        ),
        // A user the space does not let read is told there is no space.
        (
            json!({"space": x.space, "credentialKid": kid, "delegationToken": token(&x.bob, &x.space, &x.group)}),
            (400, "SpaceNotFound"),
        ),
        // A space whose authority is not here.
        (
            json!({"space": elsewhere, "credentialKid": kid, "delegationToken": token(&x.alice, &elsewhere, &x.alice.did)}),
            (400, "SpaceNotFound"),
        ),
        (
            json!({"space": x.space, "credentialKid": kid, "delegationToken": "x.y.z"}),
            (400, "InvalidDelegationToken"),
        ),
        // For another authority, another space, signed by another key, or
        // by someone with no key to be found.
        (
            json!({"space": x.space, "credentialKid": kid, "delegationToken": token(&x.alice, &x.space, &x.bob.did)}),
            (400, "InvalidDelegationToken"),
        ),
        (
            json!({"space": x.space, "credentialKid": kid, "delegationToken": token(&x.alice, &elsewhere, &x.group)}),
            (400, "InvalidDelegationToken"),
        ),
        (
            json!({"space": x.space, "credentialKid": kid, "delegationToken": token(&stranger, &x.space, &x.group)}),
            (400, "InvalidDelegationToken"),
        ),
        (
            json!({"space": x.space, "credentialKid": kid, "delegationToken": token(&unknown, &x.space, &x.group)}),
            (400, "InvalidDelegationToken"),
        ),
    ] {
        let said = issue(&x.h, body.clone()).await;
        assert_eq!(error(&said), refused, "{body}");
    }

    // A space open to all issues one to anyone, with no token.
    let open = host.create_space(&x.group, KIND, "open", true).unwrap();
    let (status, _) = issue(&x.h, json!({"space": open, "credentialKid": kid})).await;
    assert_eq!(status, 200);
}

/// `notifyWrite` for `repo` at `rev`, with `data`, signed by `who`'s key
/// as `iss` for `aud`.
async fn notify(x: &Here, who: &Keypair, iss: &str, aud: &str, body: &Json) -> (u16, Json) {
    let claims = Claims {
        iss: iss.into(),
        aud: aud.into(),
        exp: now() + 60,
        iat: Some(now()),
        lxm: Some("com.atproto.space.notifyWrite".into()),
        jti: None,
    };
    let token = service_auth::sign(who, &claims);
    let auth = [("authorization", format!("Bearer {token}"))];
    post(&x.h, "com.atproto.space.notifyWrite", body, &auth).await
}

fn rev(seconds_ahead: u64) -> String {
    atproto::tid::tid_from((now() + seconds_ahead) * 1_000_000, 1)
}

fn wrote(space: &str, repo: &str, rev: &str, data: &[u8]) -> Json {
    json!({"space": space, "repo": repo, "repoRev": rev, "data": Value::Bytes(data.to_vec()).to_json()})
}

/// The authority's credential, for `listRepos` and registering: Alice's,
/// whom the space lets read.
async fn authority_headers(x: &Here) -> Vec<(&'static str, String)> {
    let key = Keypair::generate(Curve::P256);
    let token = delegation(&x.alice, &x.space, &x.group);
    let body = json!({"space": x.space, "credentialKid": key.public().did_key(), "delegationToken": token});
    let (_, issued) = issue(&x.h, body).await;
    let authorization = format!(
        "Atproto-Space-Credential {}",
        issued["credential"].as_str().unwrap()
    );
    sign_request(&key, &authorization, &x.group)
        .unwrap()
        .into_iter()
        .collect()
}

async fn list_repos(x: &Here, rest: &str, headers: &[(&str, String)]) -> (u16, Json) {
    let got = read(
        &x.h,
        "com.atproto.space.listRepos",
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
    let (status, all) = list_repos(&x, "", &headers).await;
    assert_eq!(status, 200, "{all}");
    let repos = all["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0]["did"], x.alice.did);
    assert_eq!(repos[0]["repoRev"], first);
    assert_eq!(repos[0]["data"], Value::Bytes(vec![1; 32]).to_json());
    assert_eq!(repos[1]["did"], x.group);
    assert_eq!(repos[1]["repoRev"], commit.rev);
    assert!(repos[0]["spaceRev"].as_str() < repos[1]["spaceRev"].as_str());
    // A page at a time, after the space revision last seen.
    let (_, page) = list_repos(&x, "&limit=1", &headers).await;
    let cursor = page["cursor"].as_str().unwrap().to_string();
    assert_eq!(cursor, repos[0]["spaceRev"]);
    let (_, rest) = list_repos(&x, &format!("&cursor={cursor}"), &headers).await;
    assert_eq!(rest["repos"].as_array().unwrap().len(), 1);
    let (_, none) = list_repos(
        &x,
        &format!("&cursor={}", repos[1]["spaceRev"].as_str().unwrap()),
        &headers,
    )
    .await;
    assert_eq!(none, json!({"repos": [], "cursor": null}));
    // Only for a credential to the space.
    let (status, _) = list_repos(&x, "", &[]).await;
    assert_eq!(status, 401);

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
                    &format!("at://{}/space/{KIND}/self", x.alice.did),
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
async fn tells_the_services_registered_of_each_write() {
    let x = here().await;
    let host = &x.h.host;
    host.grant(&x.space, &x.alice.did, true, true).unwrap();
    let headers = authority_headers(&x).await;
    // Bob's account stands for a syncing service: its PDS is told.
    let register = json!({"space": x.space, "service": x.bob.did});
    let (status, registered) = post(
        &x.h,
        "com.atproto.space.registerNotify",
        &register,
        &headers,
    )
    .await;
    assert_eq!(status, 200, "{registered}");
    assert!(registered["expiresAt"].as_str().unwrap().ends_with('Z'));

    let first = host
        .space_write(&x.group, &x.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    let aud = format!("{}#atproto_pds", x.group);
    let second = rev(0);
    let body = wrote(&x.space, &x.alice.did, &second, &[4; 32]);
    notify(&x, &x.alice.key, &x.alice.did, &aud, &body).await;
    let told = told_times(&x.bob.told, 2).await;
    assert_eq!(told[0].1["repo"], x.group);
    assert_eq!(told[0].1["repoRev"], first.rev);
    assert_eq!(told[0].1.get("prevSpaceRev"), None);
    assert_eq!(told[1].1["repo"], x.alice.did);
    assert_eq!(told[1].1["prevSpaceRev"], told[0].1["spaceRev"]);
    // Signed by the group, for the service.
    let token = told[1].0.strip_prefix("Bearer ").unwrap();
    let group = host.store.account(&x.group).unwrap().unwrap();
    let claims = service_auth::verify(token, &group.signing.public(), now()).unwrap();
    assert_eq!(
        service_auth::check(&claims, &x.bob.did, "com.atproto.space.notifyWrite").unwrap(),
        x.group
    );

    // One that refuses it holds nothing up.
    x.bob.told.answer(500);
    host.space_write(&x.group, &x.space, &[create(RULE, "c", rule("C"))])
        .await
        .unwrap();
    told_times(&x.bob.told, 3).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Unregistered, it is told no more.
    let (status, _) = post(
        &x.h,
        "com.atproto.space.unregisterNotify",
        &register,
        &headers,
    )
    .await;
    assert_eq!(status, 200);
    host.space_write(&x.group, &x.space, &[create(RULE, "b", rule("B"))])
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(x.bob.told.writes.lock().unwrap().len(), 3);

    for (body, refused) in [
        (
            json!({"space": x.space, "service": "did:plc:unknown"}),
            (400, "ServiceNotResolvable"),
        ),
        (
            json!({"space": x.space, "service": format!("{}#atproto_space_syncer", x.bob.did)}),
            (400, "ServiceNotResolvable"),
        ),
        (json!({"space": x.space}), (400, "InvalidRequest")),
        (json!({"service": x.bob.did}), (400, "InvalidRequest")),
    ] {
        let said = post(&x.h, "com.atproto.space.registerNotify", &body, &headers).await;
        assert_eq!(error(&said), refused, "{body}");
    }
    let said = post(&x.h, "com.atproto.space.registerNotify", &register, &[]).await;
    assert_eq!(said.0, 401);
    let said = post(
        &x.h,
        "com.atproto.space.unregisterNotify",
        &json!({"space": x.space}),
        &headers,
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

/// What `told` was told, once it was told `times`.
async fn told_times(told: &Told, times: usize) -> Vec<(String, Json)> {
    for _ in 0..100 {
        let writes = told.writes.lock().unwrap().clone();
        if writes.len() >= times {
            return writes;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("not told {times} times");
}

#[tokio::test]
async fn a_group_writing_where_it_may_not_is_left_out() {
    let x = here().await;
    let other = x.h.host.create_account("hikers.groups.test").await.unwrap();
    let theirs = x.h.host.create_space(&other, KIND, "self", false).unwrap();
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
    let h = start(&[], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    let elsewhere = authority(&h, true, true).await;
    let space = format!("at://{}/space/{KIND}/self", elsewhere.did);
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

    // Due, and heard this time.
    elsewhere.told.answer(200);
    h.host.store.retry(&owed[0], 0).unwrap();
    assert_eq!(h.host.deliver_due().await.unwrap(), 1);
    assert_eq!(elsewhere.told.writes.lock().unwrap().len(), 2);
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
