//! The commands, run as a person on the reference PDS against hosts of
//! this repo's own, in process.

#[path = "../../group-host/tests/common/net.rs"]
#[allow(dead_code)]
mod net;

use atproto::keys::{Curve, Keypair};
use group_host::Host;
use group_host::config::Config;
use net::net;
use opensocial::{Client, Env, Host as HostAt, did_web, run};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

/// Two scopes a role may be granted: reading the moderation queue, and
/// inviting.
const MOD_READ: &str = "rpc:fyi.opensocial.listModSubjects?aud=*";
const INVITE: &str = "space:fyi.opensocial.invites?authority=*&skey=self&collection=fyi.opensocial.invite&action=read_self&action=create&action=update&action=delete";

/// A host at a free port, on the reference directory, resolving
/// handles at the reference PDS.
struct Running {
    host: Arc<Host>,
    url: String,
    favicons: String,
    _data: TempDir,
}

/// The bytes the favicon service answers for a site whose name starts
/// `chat.`.
const FAVICON: &[u8] = b"\x89PNG a green dot";

/// A favicon service, as the command asks one: where it is, the link's
/// address to follow. A site whose name starts `chat.` has `FAVICON`;
/// any other none.
async fn favicons() -> String {
    async fn icon(
        axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        let site = q.get("url").map(String::as_str).unwrap_or_default();
        match site.starts_with("https://chat.") {
            true => ([("content-type", "image/png")], FAVICON).into_response(),
            false => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = listener.local_addr().unwrap();
    let router = axum::Router::new().route("/favicon", axum::routing::get(icon));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{at}/favicon?url=")
}

async fn host() -> Running {
    host_with(&[]).await
}

/// A host at a free port, with `vars` besides.
async fn host_with(vars: &[(&str, String)]) -> Running {
    let n = net();
    // By name, as the reference PDS takes a did:web of one, not of an
    // address; on every address, IPv6 and IPv4, whichever localhost is.
    let listener = tokio::net::TcpListener::bind("[::]:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://localhost:{port}");
    let data = tempfile::tempdir().unwrap();
    let mut env: HashMap<&str, String> = HashMap::from([
        ("DATA_DIR", data.path().to_str().unwrap().to_string()),
        ("PLC_URL", n.plc.clone()),
        ("HANDLE_DOMAIN", ".groups.test".into()),
        ("HANDLE_RESOLVERS", n.pds.clone()),
        ("PUBLIC_URL", url.clone()),
    ]);
    env.extend(vars.iter().cloned());
    let host = Arc::new(Host::open(Config::from(|k| env.get(k).cloned()).unwrap()).unwrap());
    let router = group_host::router(host.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Running {
        host,
        url,
        favicons: favicons().await,
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
        ("FAVICONS", h.favicons.clone()),
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

/// What a command printed, as JSON.
fn said(printed: &str) -> Value {
    serde_json::from_str(printed).unwrap_or_else(|e| panic!("{printed}: {e}"))
}

#[tokio::test]
async fn starts_shows_and_runs_a_group() {
    let h = host().await;
    let alice = env(&h, 0);
    let bob = &net().accounts[1].handle;
    let made = ok(
        &alice,
        &[
            "group",
            "create",
            "riders.groups.test",
            "--display-name",
            "Riders",
            "--description",
            "Saturdays.",
            "--join-policy",
            "invite",
        ],
    )
    .await;
    assert!(
        made.starts_with("group    riders.groups.test did:plc:"),
        "{made}"
    );
    let did = made.split_whitespace().nth(2).unwrap().to_string();

    let seen = ok(&alice, &["group", "show", "riders.groups.test"]).await;
    assert!(seen.contains("name     Riders\n"), "{seen}");
    assert!(seen.contains("joining  invite\n"), "{seen}");
    assert!(seen.contains("admin        Admin\n"), "{seen}");
    assert!(seen.contains(&net().accounts[0].handle), "{seen}");
    assert!(seen.contains("fyi.opensocial.members/self"), "{seen}");

    // An invite, written as the group in Bob's inbox, named by his handle;
    // listed; taken back.
    let sent = ok(
        &alice,
        &["invite", "send", &did, bob, "--message", "Ride with us"],
    )
    .await;
    let uri = said(&sent)["uri"].as_str().unwrap().to_string();
    let bob_did = &net().accounts[1].did;
    assert!(
        uri.starts_with(&format!(
            "at://{bob_did}/space/fyi.opensocial.invites/self/{did}/fyi.opensocial.invite/"
        )),
        "{uri}"
    );
    let listed = said(&ok(&alice, &["invite", "list", &did, "--limit", "5"]).await);
    assert_eq!(listed["invites"][0]["uri"], uri.as_str(), "{listed}");
    ok(&alice, &["invite", "send", &did, bob, "--undo"]).await;
    let listed = said(&ok(&alice, &["invite", "list", &did]).await);
    assert_eq!(listed["invites"], json!([]), "{listed}");
    // None waiting, there is none to take back.
    let said_so = fails(&alice, &["invite", "send", &did, bob, "--undo"]).await;
    assert!(said_so.contains("InviteNotFound"), "{said_so}");

    let me = net().accounts[0].did.clone();
    let assigned = ok(&alice, &["member", "assign", &did, &me, "admin,member,"]).await;
    let membership = format!("{did}/fyi.opensocial.membership/{me}");
    assert!(
        said(&assigned)["uri"]
            .as_str()
            .unwrap()
            .ends_with(&membership)
    );
    // Who is in it, found by part of a DID.
    let listed = said(&ok(&alice, &["member", "list", &did, "--q", &me[8..]]).await);
    assert_eq!(
        listed["members"][0]["actor"]["did"],
        me.as_str(),
        "{listed}"
    );
    // Bob has not asked to join, so is given no roles; and the last who
    // runs the group is not put out: the host says why.
    let said_so = fails(&alice, &["member", "assign", &did, bob, "member"]).await;
    assert!(
        said_so.starts_with("com.atproto.space.putRecord: 400 Bad Request JoinRequestNotFound"),
        "{said_so}"
    );
    let said_so = fails(&alice, &["member", "eject", &did, &me]).await;
    assert!(
        said_so.starts_with("com.atproto.space.deleteRecord: 400 Bad Request LastConfigurer"),
        "{said_so}"
    );
    // Once he asks, he is listed, may be let in, and put out.
    h.host.store.ask(&did, bob_did, None, 0).unwrap();
    let waiting = said(&ok(&alice, &["join", "list", &did]).await);
    assert_eq!(
        waiting["requests"][0]["member"],
        bob_did.as_str(),
        "{waiting}"
    );
    ok(&alice, &["member", "assign", &did, bob, "member"]).await;
    ok(&alice, &["member", "eject", &did, bob]).await;
    // Out, he does not act as the group.
    let said_so = fails(&env(&h, 1), &["member", "eject", &did, &me]).await;
    assert!(
        said_so.starts_with("fyi.opensocial.getGroupAuth: 403"),
        "{said_so}"
    );
    // A request turned down is no request at all.
    h.host.store.ask(&did, bob_did, None, 0).unwrap();
    ok(&alice, &["join", "deny", &did, bob]).await;
    let said_so = fails(&alice, &["join", "deny", &did, bob]).await;
    assert!(said_so.contains("JoinRequestNotFound"), "{said_so}");
}

#[tokio::test]
async fn writes_the_groups_rules_roles_and_profile() {
    let h = host().await;
    let alice = env(&h, 0);
    ok(&alice, &["group", "create", "riders.groups.test"]).await;
    let group = "riders.groups.test";
    let did = h.host.store.account(group).unwrap().unwrap().did;
    let meta = group_host::group::meta_space(&did);
    let members = group_host::group::members_space(&did);
    let kept = |space: &str, kind: &str, rkey: &str| {
        let collection = format!("fyi.opensocial.{kind}");
        h.host.group_record(&did, space, &collection, rkey).unwrap()
    };

    // A rule, new at a key of the clock's; changed at it; deleted.
    let made = ok(
        &alice,
        &["rule", "set", group, "--title", "Be kind", "--text", ""],
    )
    .await;
    let uri = said(&made)["uri"].as_str().unwrap().to_string();
    let rkey = uri.rsplit('/').next().unwrap().to_string();
    assert_eq!(kept(&meta, "rule", &rkey).unwrap()["title"], "Be kind");
    let again = [
        "rule",
        "set",
        group,
        "--rule",
        &rkey,
        "--title",
        "Be very kind",
        "--text",
        "Always.",
        "--order",
        "1",
    ];
    ok(&alice, &again).await;
    let rule = kept(&meta, "rule", &rkey).unwrap();
    assert_eq!(
        (rule["title"].clone(), rule["order"].clone()),
        (json!("Be very kind"), json!(1))
    );
    ok(&alice, &["rule", "delete", group, &rkey]).await;
    assert_eq!(kept(&meta, "rule", &rkey), None);

    // A role, a bot's, with what it may do and give; added to, and back.
    let role = [
        "role",
        "set",
        group,
        "leader",
        "--display-name",
        "Ride leader",
        "--bot",
        "--no-default",
        "--scopes",
        INVITE,
        "--assignable",
        "member",
    ];
    ok(&alice, &role).await;
    let leader = || kept(&members, "role", "leader").unwrap();
    assert_eq!(
        (leader()["bot"].clone(), leader()["default"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(
        (leader()["scopes"].clone(), leader()["assignable"].clone()),
        (json!([INVITE]), json!(["member"]))
    );
    // Each added to what it had, once; one taken away, the rest kept.
    for scope in [MOD_READ, INVITE] {
        ok(
            &alice,
            &["role", "grant", group, "leader", "--scopes", scope],
        )
        .await;
    }
    assert_eq!(leader()["scopes"], json!([INVITE, MOD_READ]));
    let undo = [
        "role", "grant", group, "leader", "--scopes", MOD_READ, "--undo",
    ];
    ok(&alice, &undo).await;
    assert_eq!(leader()["scopes"], json!([INVITE]));
    // With nothing named, all of both; the rest of the role as it was.
    ok(&alice, &["role", "grant", group, "leader", "--undo"]).await;
    assert_eq!(
        (leader().get("scopes"), leader().get("assignable")),
        (None, None)
    );
    assert_eq!(leader()["displayName"], "Ride leader");
    let said_so = fails(
        &alice,
        &["role", "grant", group, "ghost", "--scopes", INVITE],
    )
    .await;
    assert!(said_so.contains("no such role: ghost"), "{said_so}");
    let said_so = fails(&alice, &["role", "delete", group, "member"]).await;
    assert!(said_so.contains("RoleInUse"), "{said_so}");
    ok(&alice, &["role", "delete", group, "leader"]).await;
    assert_eq!(kept(&members, "role", "leader"), None);

    // The profile, changed in what is given; given empty, taken away.
    let before = kept(&meta, "profile", "self").unwrap();
    let profile = [
        "profile",
        "set",
        group,
        "--display-name",
        "Riders United",
        "--topics",
        r#"[{"topic": "cycling"}]"#,
    ];
    ok(&alice, &profile).await;
    let written = kept(&meta, "profile", "self").unwrap();
    assert_eq!(written["displayName"], "Riders United");
    assert_eq!(written["topics"], json!([{"topic": "cycling"}]));
    assert_eq!(written["joinPolicy"], before["joinPolicy"]);
    let profile = ["profile", "set", group, "--description", "Bikes"];
    ok(&alice, &profile).await;
    let written = kept(&meta, "profile", "self").unwrap();
    assert_eq!(written["topics"], json!([{"topic": "cycling"}]));
    assert_eq!(written["description"], "Bikes");
    ok(&alice, &["profile", "set", group, "--topics", ""]).await;
    let written = kept(&meta, "profile", "self").unwrap();
    assert_eq!(written.get("topics"), None);
    assert_eq!(written["description"], "Bikes");
    // Its links, each new one with its site's icon where the site has
    // one, kept as the group's own; written again, each keeps it.
    let links = r#"[{"url": "https://chat.riders.example", "title": "Our chat"}, {"url": "https://riders.example"}]"#;
    let profile = ["profile", "set", group, "--links", links];
    ok(&alice, &profile).await;
    let written = kept(&meta, "profile", "self").unwrap()["links"].clone();
    assert_eq!(written[0]["icon"]["mimeType"], "image/png");
    assert_eq!(written[0]["icon"]["size"], FAVICON.len());
    assert_eq!(written[1].get("icon"), None);
    ok(&alice, &profile).await;
    assert_eq!(kept(&meta, "profile", "self").unwrap()["links"], written);

    // The queue; a label on an account, and taken back, in its history.
    let queue = said(&ok(&alice, &["mod", "list", group, "--status", "open"]).await);
    assert_eq!(queue["subjects"], json!([]), "{queue}");
    let bob = &net().accounts[1].did;
    let label = said(&ok(&alice, &["mod", "label", group, bob, "spam"]).await);
    assert!(
        label["uri"]
            .as_str()
            .unwrap()
            .contains("/fyi.opensocial.members/self/"),
        "{label}"
    );
    assert!(
        label["uri"]
            .as_str()
            .unwrap()
            .contains("/com.atproto.label.record/")
    );
    ok(&alice, &["mod", "label", group, bob, "spam", "--undo"]).await;
    let said_so = fails(&alice, &["mod", "label", group, bob, "spam", "--undo"]).await;
    assert!(said_so.contains("no spam label"), "{said_so}");
    let elsewhere = format!("at://did:plc:other/space/x.y/z/{bob}/a.b.c/d");
    let said_so = fails(&alice, &["mod", "label", group, &elsewhere, "spam"]).await;
    assert!(said_so.contains("not an account's DID"), "{said_so}");
    let history = said(&ok(&alice, &["mod", "history", group, bob]).await);
    let deleted: Vec<&Value> = history["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["event"]["deleted"])
        .collect();
    assert_eq!(deleted, [false, true], "{history}");
    let resolve = [
        "mod",
        "resolve",
        group,
        bob,
        "--status",
        "resolved",
        "--note",
        "Spoke to them",
    ];
    let resolved = said(&ok(&alice, &resolve).await);
    assert_eq!(resolved["status"], "resolved", "{resolved}");
    assert_eq!(resolved["subject"], bob.as_str(), "{resolved}");

    // A space, made as the group under the policy given, at a key of the
    // clock's.
    let policy = r#"{"grants": [{"role": "member", "read": true}]}"#;
    let make = [
        "space",
        "create",
        group,
        "com.example.calendar",
        "--policy",
        policy,
    ];
    let made = said(&ok(&alice, &make).await);
    let space = made["space"].as_str().unwrap();
    let calendars = format!("at://{did}/space/com.example.calendar/");
    assert!(space.starts_with(&calendars), "{made}");
    let kept = h.host.store.space(space).unwrap().unwrap();
    assert_eq!(kept.policy["grants"][0]["role"], "member");
}

#[tokio::test]
async fn gives_a_group_a_handle_only_on_a_domain_that_names_it() {
    let h = host().await;
    let (alice, bob) = (env(&h, 0), env(&h, 1));
    let made = ok(&alice, &["group", "create", "riders.groups.test"]).await;
    let did = made.split_whitespace().nth(2).unwrap().to_string();
    // Only one who runs it; a domain that doesn't name the group, refused
    // with what its record should say.
    let said_so = fails(&bob, &["group", "handle", &did, "riders.test"]).await;
    assert!(said_so.contains("403"), "{said_so}");
    let said_so = fails(&alice, &["group", "handle", &did, "riders.test"]).await;
    assert!(said_so.contains("NotPointing"), "{said_so}");
    assert!(said_so.contains(&format!("did={did}")), "{said_so}");
    // Its own address is always its own.
    let kept = ok(&alice, &["group", "handle", &did, "riders.groups.test"]).await;
    assert!(
        kept.contains("\"handle\": \"riders.groups.test\""),
        "{kept}"
    );
}

#[tokio::test]
async fn switches_a_group_off_and_on_and_deletes_it() {
    let h = host().await;
    let (alice, bob) = (env(&h, 0), env(&h, 1));
    let made = ok(&alice, &["group", "create", "riders.groups.test"]).await;
    let did = made.split_whitespace().nth(2).unwrap().to_string();
    // Only one who runs it, switching it off once.
    let said_so = fails(&bob, &["group", "deactivate", &did]).await;
    assert!(said_so.contains("403"), "{said_so}");
    assert_eq!(ok(&alice, &["group", "deactivate", &did]).await, "{}\n");
    let said_so = fails(&alice, &["group", "deactivate", &did]).await;
    assert!(said_so.contains("AlreadyDeactivated"), "{said_so}");
    assert!(h.host.deactivation(&did).unwrap().is_some());
    ok(&alice, &["group", "deactivate", &did, "--undo"]).await;
    assert!(h.host.deactivation(&did).unwrap().is_none());

    let said_so = fails(&bob, &["group", "delete", &did]).await;
    assert!(said_so.contains("403"), "{said_so}");
    let gone = ok(&alice, &["group", "delete", "riders.groups.test"]).await;
    assert_eq!(gone, format!("deleted {did}\nits DID is ended\n"));
    let said_so = fails(&alice, &["group", "show", &did]).await;
    assert!(said_so.contains("GroupNotFound"), "{said_so}");

    // One whose steward holds a recovery key keeps its DID, for them.
    let made = ok(&alice, &["group", "create", "tandem.groups.test"]).await;
    let did = made.split_whitespace().nth(2).unwrap().to_string();
    let person = &net().accounts[0];
    let me = Client::sign_in(&net().pds, &person.handle, &person.password);
    let me = me.await.unwrap();
    let at = HostAt {
        url: h.url.clone(),
        did: did_web(&h.url),
        favicons: String::new(),
    };
    let key = Keypair::generate(Curve::K256).public().did_key();
    let body = json!({"group": did, "key": key});
    let added = me.call(&at, "fyi.opensocial.addRecoveryKey", &body).await;
    added.unwrap();
    let gone = ok(&alice, &["group", "delete", &did]).await;
    let left = "its DID is left to the holders of its recovery keys";
    assert_eq!(gone, format!("deleted {did}\n{left}\n"));
}

#[tokio::test]
async fn seeds_a_group_to_try_things_on() {
    let h = host().await;
    let said = ok(&env(&h, 0), &["group", "seed", "trial.groups.test"]).await;
    let bob = &net().accounts[1].handle;
    assert!(said.contains("rule       Be kind\n"), "{said}");
    assert!(said.contains("role       moderator\n"), "{said}");
    assert!(said.ends_with(&format!("invited    {bob}\n")), "{said}");
    // Its rules are its own records, written as the group, in its meta.
    let trial = h.host.store.account("trial.groups.test").unwrap().unwrap();
    let meta = group_host::group::meta_space(&trial.did);
    let rules = h
        .host
        .group_records(&trial.did, &meta, "fyi.opensocial.rule")
        .unwrap();
    let titles: Vec<&str> = rules
        .iter()
        .map(|(_, r)| r["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Be kind", "Stay on topic"]);
    // Its moderator's role, which its admins give, as they give any.
    let members = group_host::group::members_space(&trial.did);
    let moderator = h
        .host
        .group_record(&trial.did, &members, "fyi.opensocial.role", "moderator")
        .unwrap()
        .unwrap();
    assert_eq!(
        moderator["scopes"],
        serde_json::json!([
            "include:fyi.opensocial.authModerate",
            "space:*?collection=com.atproto.label.record"
        ])
    );
    let h2 = host().await;
    let alice = env(&h2, 0);
    ok(&alice, &["group", "create", "quiet.groups.test"]).await;
    let group = h2
        .host
        .store
        .account("quiet.groups.test")
        .unwrap()
        .unwrap()
        .did;
    let seen = ok(&alice, &["group", "show", &group]).await;
    assert!(seen.contains("name     quiet\n"), "{seen}");
    // Someone outside it sees no members, and is told so.
    let seen = ok(&env(&h2, 1), &["group", "show", &group]).await;
    assert!(seen.contains("members\n  none\n"), "{seen}");
}

#[tokio::test]
async fn moves_a_group_to_another_host() {
    let (a, b) = (host().await, host().await);
    let alice = env(&a, 0);
    ok(&alice, &["group", "create", "riders.groups.test"]).await;
    // Only one who runs it may move it.
    let bob = env(&a, 1);
    let to = format!("{}/", b.url);
    let said = fails(&bob, &["group", "move", "riders.groups.test", "--to", &to]).await;
    assert!(said.contains("fyi.opensocial.migratePrepare"), "{said}");
    let said = ok(
        &alice,
        &[
            "group",
            "move",
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
    // Run again, it picks up at switching it on, which is done.
    let again = ["group", "move", "riders.groups.test", "--to", &b.url];
    let said = ok(&alice, &again).await;
    assert_eq!(said.lines().nth(3), Some("handed over already"), "{said}");
    assert!(said.ends_with("switched on\n"), "{said}");
    // Towards a host that never took it in, it says it is not on yet.
    let c = host().await;
    let said = fails(
        &alice,
        &["group", "move", "riders.groups.test", "--to", &c.url],
    )
    .await;
    assert!(said.contains("not switched on yet"), "{said}");
    // Named by its DID, to a host that is not there: not on yet.
    let said = fails(
        &alice,
        &[
            "group",
            "move",
            &did,
            "--to",
            "http://127.0.0.1:1",
            "--to-did",
            "did:web:nowhere",
        ],
    )
    .await;
    assert!(said.contains("reaching http://127.0.0.1:1"), "{said}");
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
    .await["space"]
    .as_str()
    .unwrap()
    .to_string();
    let alice = &n.accounts[0];
    let member =
        json!({"space": forum, "members": [{"did": alice.did, "read": true, "write": false}]});
    pds(
        "com.atproto.memberspace.putMembers",
        jwt,
        Some(member),
        None,
    )
    .await;
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
    let calendar = calendar["space"].as_str().unwrap().to_string();
    let member = json!({"space": calendar, "members": [{"did": did, "read": true, "write": true}]});
    pds(
        "com.atproto.memberspace.putMembers",
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
    let said = fails(&plain, &["account", "import", &handle]).await;
    assert!(said.contains("set OPENSOCIAL_IMPORT_PASSWORD"), "{said}");
    let said = fails(&mover, &["account", "import", &handle, "--address", "-"]).await;
    assert!(said.contains("can't move here"), "{said}");
    let said = fails(
        &mover,
        &["account", "import", &handle, "--code", "nothing-sent"],
    )
    .await;
    assert!(said.contains("no move of"), "{said}");
    // Its own label taken here, it is brought under another, and finished
    // with the code alone.
    ok(&plain, &["group", "create", &format!("{name}.groups.test")]).await;
    let said = fails(&mover, &["account", "import", &handle]).await;
    assert!(said.contains("can't move here"), "{said}");
    let club = format!("{name}-club");
    let said = ok(&mover, &["account", "import", &handle, "--address", &club]).await;
    assert!(said.contains(&format!("as   {club}.groups.test")), "{said}");
    assert!(
        said.contains("brought   3 spaces: 2 records, 1 files"),
        "{said}"
    );
    assert!(
        said.ends_with(&format!(
            "opensocial account import {handle} --code <code>\n"
        )),
        "{said}"
    );

    // With the code its server emailed, it is finished.
    let mail = reqwest::get(format!("{}?to={email}", n.mail))
        .await
        .unwrap();
    let code: Value = mail.json().await.unwrap();
    let code = code["code"].as_str().unwrap();
    let said = ok(&mover, &["account", "import", &handle, "--code", code]).await;
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
    // Its records in each space; in the forum, which is now a group's, its
    // policy too, published. Alice's calendar has hers.
    let paths = store.space_paths(did, &forum).unwrap();
    assert_eq!(
        paths.keys().collect::<Vec<_>>(),
        ["com.atproto.space.policy/self", "com.example.note/ride"]
    );
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
        &["group", "create"][..],
        &["group", "show", "a", "b"],
        &["group", "move", "riders.groups.test"],
        &["group", "delete"],
        &["member", "assign", "g"],
        &["member", "eject", "g", "m", "extra"],
        &["member"],
    ] {
        let said = fails(&alice, args).await;
        assert!(
            said.starts_with("usage:") || said.starts_with("no command member\n"),
            "{args:?}: {said}"
        );
    }
    assert!(
        fails(&alice, &["group", "create", "x", "--colour", "red"])
            .await
            .starts_with("no option --colour")
    );
    assert!(
        fails(
            &alice,
            &["member", "assign", "g", "m", "r", "--reason", "x"]
        )
        .await
        .starts_with("no option --reason")
    );
    assert_eq!(
        fails(&alice, &["group", "create", "x", "--display-name"]).await,
        "--display-name wants a value"
    );
    assert!(
        fails(&alice, &["frob"])
            .await
            .starts_with("no command frob\n")
    );
    assert!(
        fails(&alice, &["member", "frob", "g", "m"])
            .await
            .starts_with("no command member frob\n")
    );
    for args in [&[][..], &["help"], &["--help"], &["-h"]] {
        assert_eq!(ok(&alice, args).await, opensocial::USAGE);
    }

    let mut nobody = alice.clone();
    nobody.password = None;
    assert!(
        fails(&nobody, &["group", "show", "x"])
            .await
            .contains("set OPENSOCIAL_AS and OPENSOCIAL_PASSWORD")
    );
    let mut wrong = alice.clone();
    wrong.password = Some("not it".into());
    assert!(
        fails(&wrong, &["group", "show", "x"])
            .await
            .starts_with("signing in as ")
    );
    let mut unreachable = alice.clone();
    unreachable.pds = "http://127.0.0.1:1".into();
    assert!(
        fails(&unreachable, &["group", "show", "x"])
            .await
            .starts_with("reaching http://127.0.0.1:1")
    );
    let mut gone = alice.clone();
    gone.host = "http://127.0.0.1:1".into();
    for args in [
        &["group", "show", "x.test"][..],
        &["group", "create", "x.groups.test"],
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

#[tokio::test]
async fn publishes_a_folder_as_the_groups_site_and_takes_it_down() {
    let h = host().await;
    let alice = env(&h, 0);
    let group = "riders.groups.test";
    ok(&alice, &["group", "create", group]).await;
    let did = h.host.store.account(group).unwrap().unwrap().did;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("rides")).unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join("index.html"), "<h1>Riders</h1>").unwrap();
    std::fs::write(dir.path().join("rides/index.html"), "<h1>Rides</h1>").unwrap();
    std::fs::write(dir.path().join("logo.png"), [0x89, b'P', b'N', b'G', 0xff]).unwrap();
    std::fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
    std::fs::write(dir.path().join(".git/HEAD"), "ref").unwrap();
    let folder = dir.path().to_str().unwrap();

    let args = [
        "site",
        "publish",
        group,
        folder,
        "--not-found",
        "index.html",
        "--fallback",
        "rides/index.html",
    ];
    let made = said(&ok(&alice, &args).await);
    assert_eq!(made["uploaded"], 3, "{made}");
    assert_eq!(made["url"], format!("http://{group}/"));
    let record = h
        .host
        .store
        .record(&did, "fyi.opensocial.site", "self")
        .unwrap()
        .unwrap();
    let site = atproto::drisl::decode(&record.value).unwrap().to_json();
    let paths: Vec<&str> = site["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["index.html", "logo.png", "rides/index.html"]);
    assert_eq!(
        (site["notFound"].as_str(), site["fallback"].as_str()),
        (Some("index.html"), Some("rides/index.html"))
    );

    // Reads by name, as one of the host's, or by NSID.
    let args = [
        "site",
        "publish",
        group,
        folder,
        "--reads",
        "getGroup, fyi.opensocial.listMembers",
    ];
    ok(&alice, &args).await;
    let record = h.host.store.record(&did, "fyi.opensocial.site", "self");
    let site = atproto::drisl::decode(&record.unwrap().unwrap().value).unwrap();
    let reads = &site.to_json()["reads"];
    let wanted =
        json!([{"method": "fyi.opensocial.getGroup"}, {"method": "fyi.opensocial.listMembers"}]);
    assert_eq!(reads, &wanted);

    // Writes, each a type of the group's spaces and a collection there.
    let args = [
        "site",
        "publish",
        group,
        folder,
        "--writes",
        "com.example.board:com.example.note, fyi.opensocial.members:fyi.opensocial.acceptance",
    ];
    ok(&alice, &args).await;
    let record = h.host.store.record(&did, "fyi.opensocial.site", "self");
    let site = atproto::drisl::decode(&record.unwrap().unwrap().value).unwrap();
    let wanted = json!([
        {"space": "com.example.board", "collection": "com.example.note"},
        {"space": "fyi.opensocial.members", "collection": "fyi.opensocial.acceptance"},
    ]);
    assert_eq!(&site.to_json()["writes"], &wanted);
    let unsaid = fails(
        &alice,
        &[
            "site",
            "publish",
            group,
            folder,
            "--writes",
            "com.example.note",
        ],
    )
    .await;
    assert!(
        unsaid.contains("a write is <type>:<collection> or <type>/<key>:<collection>"),
        "{unsaid}"
    );
    // A write in just the one space of a type, by its key.
    let args = [
        "site",
        "publish",
        group,
        folder,
        "--writes",
        "fyi.opensocial.siteSpace/board:com.example.note",
    ];
    ok(&alice, &args).await;
    let record = h.host.store.record(&did, "fyi.opensocial.site", "self");
    let site = atproto::drisl::decode(&record.unwrap().unwrap().value).unwrap();
    let wanted = json!([
        {"space": "fyi.opensocial.siteSpace", "key": "board", "collection": "com.example.note"},
    ]);
    assert_eq!(&site.to_json()["writes"], &wanted);

    // Spaces, each a type by its NSID, read whole by the site's pages.
    let args = [
        "site",
        "publish",
        group,
        folder,
        "--spaces",
        "com.example.board, fyi.opensocial.members",
    ];
    ok(&alice, &args).await;
    let record = h.host.store.record(&did, "fyi.opensocial.site", "self");
    let site = atproto::drisl::decode(&record.unwrap().unwrap().value).unwrap();
    let wanted = json!([{"space": "com.example.board"}, {"space": "fyi.opensocial.members"}]);
    assert_eq!(&site.to_json()["spaces"], &wanted);
    // Or just the one space of a type, by its key.
    let args = [
        "site",
        "publish",
        group,
        folder,
        "--spaces",
        "fyi.opensocial.siteSpace/board, fyi.opensocial.siteSpace/calendar",
    ];
    ok(&alice, &args).await;
    let record = h.host.store.record(&did, "fyi.opensocial.site", "self");
    let site = atproto::drisl::decode(&record.unwrap().unwrap().value).unwrap();
    let wanted = json!([
        {"space": "fyi.opensocial.siteSpace", "key": "board"},
        {"space": "fyi.opensocial.siteSpace", "key": "calendar"},
    ]);
    assert_eq!(&site.to_json()["spaces"], &wanted);
    let keyless = [
        "site",
        "publish",
        group,
        folder,
        "--spaces",
        "fyi.opensocial.siteSpace/",
    ];
    let keyless = fails(&alice, &keyless).await;
    assert!(
        keyless.contains("a space is <type> or <type>/<key>"),
        "{keyless}"
    );

    let missing = fails(&alice, &["site", "publish", group, "/no/such/folder"]).await;
    assert!(missing.contains("reading /no/such/folder"), "{missing}");

    // Its handle its address, there is no domain to move it to; moving it
    // back is the opposite method, and leaves it where it is.
    let moved = fails(&alice, &["site", "domain", group]).await;
    assert!(moved.contains("NoOwnDomain"), "{moved}");
    ok(&alice, &["site", "domain", group, "--undo"]).await;

    ok(&alice, &["site", "delete", group]).await;
    assert!(
        h.host
            .store
            .record(&did, "fyi.opensocial.site", "self")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn reads_the_hosts_logs_and_stats_as_its_operator() {
    let h = host_with(&[("HOST_ADMINS", net().accounts[0].did.clone())]).await;
    let (alice, bob) = (env(&h, 0), env(&h, 1));
    ok(
        &alice,
        &[
            "group",
            "create",
            "riders.groups.test",
            "--display-name",
            "Riders",
        ],
    )
    .await;
    let line = json!({"level": "INFO", "fields": {"message": "request"},
        "spans": [{"name": "request", "trace": "abcd"}]});
    h.host.logs.keep(format!("{line}\n").as_bytes());

    let printed = ok(
        &alice,
        &["admin", "logs", "--trace", "abcd", "--min-ms", "0"],
    )
    .await;
    assert_eq!(printed, "");
    let printed = ok(&alice, &["admin", "logs", "--trace", "abcd"]).await;
    assert_eq!(said(&printed), line);
    let printed = ok(&alice, &["admin", "stats", "--days", "2"]).await;
    let mut printed = printed.lines();
    assert_eq!(
        printed.next(),
        Some("host\t1 groups, 0 switched off, 2 spaces, 0 bytes of files")
    );
    assert!(printed.next().unwrap().starts_with("day\trequests"));
    printed.next();
    let today = printed.next().unwrap();
    assert!(today.ends_with("\t0\t1\t2"), "{today}");
    let rest: Vec<&str> = printed.collect();
    let made = "route\tPOST /xrpc/fyi.opensocial.createGroup\t1 requests, 0 refused, 0 failed";
    assert!(rest.iter().any(|l| l.starts_with(made)), "{rest:?}");

    // Only the host's operators may read them.
    let refused = fails(&bob, &["admin", "stats"]).await;
    assert!(refused.contains("Forbidden"), "{refused}");
    let refused = fails(&alice, &["admin", "logs", "--since", "now"]).await;
    assert!(refused.starts_with("no option --since"), "{refused}");
    let refused = fails(&alice, &["admin", "stats", "today"]).await;
    assert!(refused.starts_with("usage:"), "{refused}");
}

#[tokio::test]
async fn prints_the_tools_the_host_counted() {
    let h = host_with(&[("HOST_ADMINS", net().accounts[0].did.clone())]).await;
    h.host.stats.tool("member_list", false);
    let printed = ok(&env(&h, 0), &["admin", "stats", "--days", "1"]).await;
    let last = printed.lines().last().unwrap();
    assert_eq!(last, "tool\tmember_list\t1 calls, 1 failed");
}
