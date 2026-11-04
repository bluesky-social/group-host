//! The commands, run as a person on the reference PDS against hosts of
//! this repo's own, in process.

#[path = "../../group-host/tests/common/net.rs"]
#[allow(dead_code)]
mod net;

use group_host::Host;
use group_host::config::Config;
use net::net;
use opensocial::{Env, did_web, run};
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

#[test]
fn reads_its_environment() {
    let env = Env::from(|_| None);
    assert_eq!(env.host, "http://localhost:2600");
    assert_eq!(env.host_did, "did:web:localhost%3A2600");
    assert_eq!(env.pds, "http://localhost:2583");
    assert!(env.handle.is_none() && env.seed_invite.is_empty());
    assert_eq!(did_web("https://groups.example/"), "did:web:groups.example");
}
