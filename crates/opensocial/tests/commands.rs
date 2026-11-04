//! The commands, run as a person on the reference PDS against hosts of
//! this repo's own, in process.

#[path = "../../group-host/tests/common/net.rs"]
#[allow(dead_code)]
mod net;

use group_host::Host;
use group_host::config::Config;
use net::net;
use opensocial::{Env, did_web, run};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

/// A host at a free port, on the reference directory, resolving
/// handles at the reference PDS.
struct Running {
    host: Arc<Host>,
    url: String,
    _data: TempDir,
}

async fn host() -> Running {
    let n = net();
    // By name, as the reference PDS takes a did:web of one, not of an
    // address; on every address, IPv6 and IPv4, whichever localhost is.
    let listener = tokio::net::TcpListener::bind("[::]:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://localhost:{port}");
    let data = tempfile::tempdir().unwrap();
    let env: HashMap<&str, String> = HashMap::from([
        ("DATA_DIR", data.path().to_str().unwrap().to_string()),
        ("PLC_URL", n.plc.clone()),
        ("HANDLE_DOMAIN", ".groups.test".into()),
        ("HANDLE_RESOLVERS", n.pds.clone()),
        ("PUBLIC_URL", url.clone()),
    ]);
    let host = Arc::new(Host::open(Config::from(|k| env.get(k).cloned()).unwrap()).unwrap());
    let router = group_host::router(host.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Running {
        host,
        url,
        _data: data,
    }
}

/// The command's environment: `h`, signed in as the reference PDS's
/// `who`th account.
fn env(h: &Running, who: usize) -> Env {
    let n = net();
    let person = &n.accounts[who];
    let vars: HashMap<&str, String> = HashMap::from([
        ("OPENSOCIAL_HOST", format!("{}/", h.url)),
        ("PDS_URL", n.pds.clone()),
        ("OPENSOCIAL_AS", person.handle.clone()),
        ("OPENSOCIAL_PASSWORD", person.password.clone()),
        (
            "OPENSOCIAL_SEED_INVITE",
            format!(" {} ,", n.accounts[1].handle),
        ),
    ]);
    Env::from(|k| vars.get(k).cloned())
}

async fn opensocial(env: &Env, args: &[&str]) -> anyhow::Result<String> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    run(&args, env).await
}

async fn ok(env: &Env, args: &[&str]) -> String {
    opensocial(env, args)
        .await
        .unwrap_or_else(|e| panic!("{args:?}: {e:#}"))
}

async fn fails(env: &Env, args: &[&str]) -> String {
    format!("{:#}", opensocial(env, args).await.unwrap_err())
}

#[tokio::test]
async fn starts_inspects_and_runs_a_group() {
    let h = host().await;
    let alice = env(&h, 0);
    let bob = &net().accounts[1].handle;
    let made = ok(
        &alice,
        &[
            "create-group",
            "riders.groups.test",
            "--name",
            "Riders",
            "--description",
            "Saturdays.",
            "--join",
            "invite",
        ],
    )
    .await;
    assert!(
        made.starts_with("group    riders.groups.test did:plc:"),
        "{made}"
    );
    let did = made.split_whitespace().nth(2).unwrap().to_string();

    let seen = ok(&alice, &["inspect", "riders.groups.test"]).await;
    assert!(seen.contains("name     Riders\n"), "{seen}");
    assert!(seen.contains("joining  invite\n"), "{seen}");
    assert!(seen.contains("admin        Admin\n"), "{seen}");
    assert!(seen.contains(&net().accounts[0].handle), "{seen}");
    assert!(seen.contains("fyi.opensocial.members/self"), "{seen}");

    let said = ok(
        &alice,
        &[
            "membership",
            "invite",
            &did,
            bob,
            "--message",
            "Ride with us",
        ],
    )
    .await;
    assert_eq!(said, format!("{bob}: invited\n"));
    let me = net().accounts[0].did.clone();
    let said = ok(
        &alice,
        &["membership", "assign", &did, &me, "admin,member,"],
    )
    .await;
    assert_eq!(said, format!("{me}: admin, member\n"));
    // Bob is no member yet: the host says why.
    let said = fails(
        &alice,
        &["membership", "eject", &did, bob, "--reason", "spam"],
    )
    .await;
    assert!(
        said.starts_with("fyi.opensocial.ejectMember: 400"),
        "{said}"
    );
}

#[tokio::test]
async fn seeds_a_group_to_try_things_on() {
    let h = host().await;
    let said = ok(&env(&h, 0), &["seed", "trial.groups.test"]).await;
    let bob = &net().accounts[1].handle;
    assert!(said.contains("rule       Be kind\n"), "{said}");
    assert!(said.contains("role       moderator\n"), "{said}");
    assert!(said.ends_with(&format!("invited    {bob}\n")), "{said}");
    let h2 = host().await;
    let alice = env(&h2, 0);
    ok(&alice, &["create-group", "quiet.groups.test"]).await;
    let group = h2
        .host
        .store
        .account("quiet.groups.test")
        .unwrap()
        .unwrap()
        .did;
    let seen = ok(&alice, &["inspect", &group]).await;
    assert!(seen.contains("name     quiet\n"), "{seen}");
    // Someone outside it sees no members, and is told so.
    let seen = ok(&env(&h2, 1), &["inspect", &group]).await;
    assert!(seen.contains("members\n  none\n"), "{seen}");
}

#[tokio::test]
async fn moves_a_group_to_another_host() {
    let (a, b) = (host().await, host().await);
    let alice = env(&a, 0);
    ok(&alice, &["create-group", "riders.groups.test"]).await;
    let said = ok(
        &alice,
        &[
            "migrate",
            "riders.groups.test",
            "--to",
            &format!("{}/", b.url),
        ],
    )
    .await;
    assert!(said.contains("handed over to "), "{said}");
    assert!(said.ends_with("switched on\n"), "{said}");
    let did = a
        .host
        .store
        .account("riders.groups.test")
        .unwrap()
        .unwrap()
        .did;
    assert!(b.host.deactivation(&did).unwrap().is_none());
    // Named by its DID, to a host that is not there.
    let said = fails(
        &alice,
        &[
            "migrate",
            &did,
            "--to",
            "http://127.0.0.1:1",
            "--to-did",
            "did:web:nowhere",
        ],
    )
    .await;
    assert!(said.contains("fyi.opensocial.migratePrepare"), "{said}");
}

/// `nsid` of the reference PDS, as `bearer`: a query with `query`, or a
/// procedure with `body`; `bytes` of `kind` for one that takes them.
async fn pds(nsid: &str, bearer: &str, body: Option<Value>, bytes: Option<(&[u8], &str)>) -> Value {
    let url = format!("{}/xrpc/{nsid}", net().pds);
    let http = reqwest::Client::new();
    let req = match (body, bytes) {
        (_, Some((b, kind))) => http.post(url).header("content-type", kind).body(b.to_vec()),
        (Some(body), None) => http.post(url).json(&body),
        (None, None) => http.get(url),
    };
    let req = if bearer.is_empty() {
        req
    } else {
        req.bearer_auth(bearer)
    };
    let res = req.send().await.unwrap();
    let status = res.status();
    let said: Value = res.json().await.unwrap_or(Value::Null);
    assert!(status.is_success(), "{nsid}: {status} {said}");
    said
}

#[tokio::test]
async fn moves_an_account_and_its_spaces_here_to_become_a_group() {
    let n = net();
    if !n.spaces {
        eprintln!("the reference PDS has no spaces: GROUP_HOST_DEV_ENV names the alpha's build");
        return;
    }
    let h = host().await;
    // An account of its own on the reference PDS, which moves away.
    let name = format!("harbor{}", std::process::id());
    let (handle, email, password) = (
        format!("{name}.test"),
        format!("{name}@test.com"),
        "harbor-password",
    );
    let made = pds(
        "com.atproto.server.createAccount",
        "",
        Some(json!({"handle": handle, "email": email, "password": password})),
        None,
    )
    .await;
    let (did, jwt) = (
        made["did"].as_str().unwrap(),
        made["accessJwt"].as_str().unwrap(),
    );
    // A forum it governs, which Alice reads, holding a photo; and a space
    // of its own it has written nothing in.
    let forum = pds(
        "com.atproto.space.createSpace",
        jwt,
        Some(json!({"spaceType": "com.example.forum", "skey": "general", "policy": {"$type": "com.atproto.memberspace.policy", "publicRead": true}})),
        None,
    )
    .await["uri"]
    .as_str()
    .unwrap()
    .to_string();
    let alice = &n.accounts[0];
    let member = json!({"space": forum, "did": alice.did, "read": true, "write": false});
    pds("com.atproto.memberspace.putMember", jwt, Some(member), None).await;
    let photo = pds(
        "com.atproto.repo.uploadBlob",
        jwt,
        None,
        Some((&[1, 2, 3], "image/png")),
    )
    .await;
    let note = json!({"$type": "com.example.note", "text": "Saturday's ride", "photo": photo["blob"], "createdAt": "2026-11-04T00:00:00Z"});
    let record = json!({"space": forum, "repo": did, "collection": "com.example.note", "rkey": "ride", "record": note});
    pds("com.atproto.space.createRecord", jwt, Some(record), None).await;
    let diary = json!({"spaceType": "com.example.diary", "skey": "self", "policy": {"$type": "com.atproto.personalspace.policy"}});
    pds("com.atproto.space.createSpace", jwt, Some(diary), None).await;
    // And a calendar of Alice's it writes in.
    let session = json!({"identifier": alice.handle, "password": alice.password});
    let session = pds("com.atproto.server.createSession", "", Some(session), None).await;
    let hers = session["accessJwt"].as_str().unwrap();
    let calendar = json!({"spaceType": "com.example.calendar", "skey": name, "policy": {"$type": "com.atproto.memberspace.policy"}});
    let calendar = pds("com.atproto.space.createSpace", hers, Some(calendar), None).await;
    let calendar = calendar["uri"].as_str().unwrap().to_string();
    let member = json!({"space": calendar, "did": did, "read": true, "write": true});
    pds(
        "com.atproto.memberspace.putMember",
        hers,
        Some(member),
        None,
    )
    .await;
    let event = json!({"$type": "com.example.note", "text": "Ride on Saturday", "createdAt": "2026-11-04T00:00:00Z"});
    let record = json!({"space": calendar, "repo": did, "collection": "com.example.note", "rkey": "saturday", "record": event});
    pds("com.atproto.space.createRecord", jwt, Some(record), None).await;

    // Alice moves it, with its own password.
    let plain = env(&h, 0);
    let mut mover = plain.clone();
    mover.import_password = Some(password.into());
    let said = fails(&plain, &["import", &handle]).await;
    assert!(said.contains("set OPENSOCIAL_IMPORT_PASSWORD"), "{said}");
    let said = fails(&mover, &["import", &handle, "--address", "-"]).await;
    assert!(said.contains("can't move here"), "{said}");
    let said = fails(&mover, &["import", &handle, "--code", "nothing-sent"]).await;
    assert!(said.contains("no move of"), "{said}");
    let club = format!("{name}-club");
    let said = ok(&mover, &["import", &handle, "--address", &club]).await;
    assert!(said.contains(&format!("as   {club}.groups.test")), "{said}");
    assert!(
        said.contains("brought   3 spaces: 2 records, 1 files"),
        "{said}"
    );
    assert!(
        said.ends_with(&format!("opensocial import {handle} --code <code>\n")),
        "{said}"
    );

    // With the code its server emailed, it is finished.
    let mail = reqwest::get(format!("{}?to={email}", n.mail))
        .await
        .unwrap();
    let code: Value = mail.json().await.unwrap();
    let code = code["code"].as_str().unwrap();
    let said = ok(&mover, &["import", &handle, "--code", code]).await;
    assert_eq!(
        said,
        format!("moved: {club}.groups.test is a group here, run by you\n")
    );
    let store = &h.host.store;
    assert!(h.host.deactivation(did).unwrap().is_none());
    assert!(store.space(&forum).unwrap().unwrap().public_read);
    assert_eq!(
        store.granted(&forum, &alice.did).unwrap(),
        Some((true, false))
    );
    let paths = store.space_paths(did, &forum).unwrap();
    assert_eq!(paths.keys().collect::<Vec<_>>(), ["com.example.note/ride"]);
    let held = store.space_paths(did, &calendar).unwrap();
    assert_eq!(
        held.keys().collect::<Vec<_>>(),
        ["com.example.note/saturday"]
    );
    let writers = store.writers(&forum, None, 10).unwrap();
    assert_eq!(
        writers.iter().map(|w| w.did.as_str()).collect::<Vec<_>>(),
        [did]
    );
}

#[tokio::test]
async fn says_what_is_wrong_with_what_it_is_asked() {
    let h = host().await;
    let alice = env(&h, 0);
    for args in [
        &["create-group"][..],
        &["inspect", "a", "b"],
        &["migrate", "riders.groups.test"],
        &["membership"],
        &["membership", "assign", "g"],
        &["membership", "frob", "g", "m"],
        &["membership", "eject", "g", "m", "extra"],
    ] {
        assert!(fails(&alice, args).await.starts_with("usage:"), "{args:?}");
    }
    assert!(
        fails(&alice, &["create-group", "x", "--colour", "red"])
            .await
            .starts_with("no option --colour")
    );
    assert!(
        fails(
            &alice,
            &["membership", "assign", "g", "m", "r", "--reason", "x"]
        )
        .await
        .starts_with("no option --reason")
    );
    assert_eq!(
        fails(&alice, &["create-group", "x", "--name"]).await,
        "--name wants a value"
    );
    assert!(
        fails(&alice, &["frob"])
            .await
            .starts_with("no command frob")
    );
    for args in [&[][..], &["help"], &["--help"], &["-h"]] {
        assert_eq!(ok(&alice, args).await, opensocial::USAGE);
    }

    let mut nobody = alice.clone();
    nobody.password = None;
    assert!(
        fails(&nobody, &["inspect", "x"])
            .await
            .contains("set OPENSOCIAL_AS and OPENSOCIAL_PASSWORD")
    );
    let mut wrong = alice.clone();
    wrong.password = Some("not it".into());
    assert!(
        fails(&wrong, &["inspect", "x"])
            .await
            .starts_with("signing in as ")
    );
    let mut unreachable = alice.clone();
    unreachable.pds = "http://127.0.0.1:1".into();
    assert!(
        fails(&unreachable, &["inspect", "x"])
            .await
            .starts_with("reaching http://127.0.0.1:1")
    );
    let mut gone = alice.clone();
    gone.host = "http://127.0.0.1:1".into();
    for args in [
        &["inspect", "x.test"][..],
        &["create-group", "x.groups.test"],
    ] {
        let said = fails(&gone, args).await;
        assert!(said.starts_with("reaching http://127.0.0.1:1"), "{said}");
    }
}

/// An app acting as a person reaches the host through the person's own
/// PDS, as `atproto-proxy` names the host's `#opensocial` service: the
/// PDS signs the call for them, and the host answers it as theirs.
#[tokio::test]
async fn answers_a_call_a_persons_pds_proxies() {
    let h = host().await;
    let n = net();
    let alice = &n.accounts[0];
    let http = reqwest::Client::new();
    let session: serde_json::Value = http
        .post(format!("{}/xrpc/com.atproto.server.createSession", n.pds))
        .json(&serde_json::json!({"identifier": alice.handle, "password": alice.password}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let res = http
        .get(format!("{}/xrpc/fyi.opensocial.getViewer", n.pds))
        .header("atproto-proxy", format!("{}#opensocial", did_web(&h.url)))
        .bearer_auth(session["accessJwt"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    let status = res.status();
    let said: serde_json::Value = res.json().await.unwrap_or_default();
    assert!(status.is_success(), "{status} {said}");
    assert_eq!(said["actor"]["did"], alice.did.as_str(), "{said}");
}

#[test]
fn reads_its_environment() {
    let env = Env::from(|_| None);
    assert_eq!(env.host, "http://localhost:2600");
    assert_eq!(env.host_did, "did:web:localhost%3A2600");
    assert_eq!(env.pds, "http://localhost:2583");
    assert!(env.handle.is_none() && env.seed_invite.is_empty());
    assert_eq!(did_web("https://groups.example/"), "did:web:groups.example");
}
