//! Groups' accounts and repos: made on the PLC directory, written a
//! signed commit at a time, and read back as a PDS answers them.

mod common;

use atproto::mst::Tree;
use atproto::{Commit, PublicKey, car};
use common::start;
use group_host::repo::Write;
use group_host::store::{Account, Head, Store};
use group_host::xrpc::XrpcError;
use serde_json::{Value, json};

const RIDERS: &str = "riders.groups.test";

fn create(rkey: &str, text: &str) -> Write {
    Write::Create {
        collection: "fyi.opensocial.rule".into(),
        rkey: rkey.into(),
        value: json!({"$type": "fyi.opensocial.rule", "title": text, "createdAt": "2026-11-03T00:00:00Z"}),
    }
}

#[tokio::test]
async fn makes_a_group_account_on_the_directory() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    assert!(did.starts_with("did:plc:"));

    // Its genesis, signed with the host's rotation key, naming the host
    // as its PDS and its account's own signing key.
    let ops = h.plc.ops.lock().unwrap()[&did].clone();
    assert_eq!(ops.len(), 1);
    let rotation = h.host.secrets.rotation.public().did_key();
    assert_eq!(ops[0].signed_by(&[rotation]), Some(0));

    let (status, repo) = h
        .get("com.atproto.repo.describeRepo", &format!("repo={RIDERS}"))
        .await;
    assert_eq!(status, 200, "{repo}");
    assert_eq!(repo["did"], did.as_str());
    assert_eq!(repo["handle"], RIDERS);
    assert_eq!(repo["collections"], json!([]));
    assert_eq!(
        atproto::did::pds(&repo["didDoc"]).as_deref(),
        Some("https://host.test")
    );
    let account = h.host.store.account(&did).unwrap().unwrap();
    assert_eq!(
        atproto::did::signing_key(&repo["didDoc"]).unwrap(),
        account.signing.public()
    );
    // And the key a space's syncer agrees a secret with.
    assert_eq!(
        atproto::did::kem_key(&repo["didDoc"]).unwrap(),
        account.kem.public()
    );

    let (_, resolved) = h
        .get(
            "com.atproto.identity.resolveHandle",
            &format!("handle={RIDERS}"),
        )
        .await;
    assert_eq!(resolved["did"], did.as_str());
    let by_host = h
        .http
        .get(format!("{}/.well-known/atproto-did", h.url))
        .header("host", "Riders.groups.test:443")
        .send()
        .await
        .unwrap();
    assert_eq!(by_host.text().await.unwrap(), did);

    // An empty repo, its first commit signed by the account.
    let (_, latest) = h
        .get("com.atproto.sync.getLatestCommit", &format!("did={did}"))
        .await;
    let (_, repos) = h.get("com.atproto.sync.listRepos", "").await;
    assert_eq!(
        repos["repos"],
        json!([{"did": did, "head": latest["cid"], "rev": latest["rev"], "active": true}])
    );
    assert_eq!(repos["cursor"], Value::Null);
}

#[tokio::test]
async fn keeps_a_handle_in_lower_case() {
    let h = start(&[], false).await;
    let did = h.host.create_account("Riders.Groups.Test").await.unwrap();
    let account = h.host.store.account(&did).unwrap().unwrap();
    assert_eq!(account.handle, RIDERS);
    let err = h.host.create_account(RIDERS).await.unwrap_err();
    assert_eq!(err.error, "HandleNotAvailable");
    let (_, resolved) = h
        .get(
            "com.atproto.identity.resolveHandle",
            "handle=RIDERS.groups.test",
        )
        .await;
    assert_eq!(resolved["did"], did.as_str());
    let (_, repo) = h
        .get("com.atproto.repo.describeRepo", "repo=Riders.groups.test")
        .await;
    assert_eq!(repo["did"], did.as_str());
    let ops = h.plc.ops.lock().unwrap()[&did].clone();
    let atproto::plc::Signed::Update { op, .. } = &ops[0] else {
        panic!()
    };
    assert_eq!(op.handle(), Some(RIDERS));
}

#[tokio::test]
async fn refuses_handles_it_cannot_give() {
    let h = start(&[], false).await;
    for bad in [
        "riders.example.com",
        "a.riders.groups.test",
        ".groups.test",
        "-riders.groups.test",
        "groups.test",
    ] {
        let err = h.host.create_account(bad).await.unwrap_err();
        assert_eq!(err.error, "InvalidHandle", "{bad}");
    }
    h.host.create_account(RIDERS).await.unwrap();
    assert_eq!(
        h.host.create_account(RIDERS).await.unwrap_err().error,
        "HandleNotAvailable"
    );

    let refused = start(&[], true).await;
    let err = refused.host.create_account(RIDERS).await.unwrap_err();
    assert_eq!(err.error, "AccountCreationFailed");
    assert!(err.message.contains("refused"), "{}", err.message);
    assert!(refused.host.store.account(RIDERS).unwrap().is_none());
}

#[tokio::test]
async fn writes_records_as_signed_commits() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let first = h
        .host
        .write(
            &did,
            &[
                create("a", "Ride safe"),
                create("b", "Be kind"),
                create("c", "No spam"),
            ],
        )
        .unwrap();
    let second = h
        .host
        .write(
            &did,
            &[
                Write::Update {
                    collection: "fyi.opensocial.rule".into(),
                    rkey: "b".into(),
                    value: json!({"$type": "fyi.opensocial.rule", "title": "Be kinder", "createdAt": "2026-11-03T00:00:00Z"}),
                },
                Write::Delete { collection: "fyi.opensocial.rule".into(), rkey: "c".into() },
            ],
        )
        .unwrap();
    assert!(second.rev > first.rev);

    let (status, b) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={did}&collection=fyi.opensocial.rule&rkey=b"),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(b["value"]["title"], "Be kinder");
    assert_eq!(b["uri"], format!("at://{did}/fyi.opensocial.rule/b"));
    let cid = b["cid"].as_str().unwrap();
    let (status, _) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={RIDERS}&collection=fyi.opensocial.rule&rkey=b&cid={cid}"),
        )
        .await;
    assert_eq!(status, 200);
    let (status, gone) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={did}&collection=fyi.opensocial.rule&rkey=c"),
        )
        .await;
    assert_eq!(
        (status, gone["error"].as_str()),
        (400, Some("RecordNotFound"))
    );
    let (status, _) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={did}&collection=fyi.opensocial.rule&rkey=a&cid={cid}"),
        )
        .await;
    assert_eq!(status, 400);

    // Newest key first, a page at a time; oldest first with reverse.
    let lists = "com.atproto.repo.listRecords";
    let list = |q: &str| format!("repo={did}&collection=fyi.opensocial.rule&{q}");
    let (_, page) = h.get(lists, &list("limit=1")).await;
    assert_eq!(page["records"][0]["value"]["title"], "Be kinder");
    assert_eq!(page["cursor"], "b");
    let (_, page) = h.get(lists, &list("limit=1&cursor=b")).await;
    assert_eq!(page["records"][0]["value"]["title"], "Ride safe");
    let (_, page) = h.get(lists, &list("reverse=true")).await;
    let titles: Vec<&Value> = page["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["value"]["title"])
        .collect();
    assert_eq!(titles, [&json!("Ride safe"), &json!("Be kinder")]);
    assert_eq!(page["cursor"], Value::Null);
    for bad in ["limit=0", "limit=101", "limit=x"] {
        assert_eq!(
            h.get(lists, &list(bad)).await.1["message"],
            "limit is 1 to 100"
        );
    }
    let (_, repo) = h
        .get("com.atproto.repo.describeRepo", &format!("repo={did}"))
        .await;
    assert_eq!(repo["collections"], json!(["fyi.opensocial.rule"]));

    // The whole repo, as a CAR: the commit first, signed by the account,
    // and the tree its keys make, with every record.
    let res = h
        .http
        .get(format!("{}/xrpc/com.atproto.sync.getRepo?did={did}", h.url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.headers()["content-type"], "application/vnd.ipld.car");
    let car = car::read(&res.bytes().await.unwrap()).unwrap();
    let commit = Commit::from_bytes(&car.blocks[&car.roots[0]]).unwrap();
    assert_eq!(commit.cid().to_string(), second.cid.to_string());
    let (_, latest) = h
        .get("com.atproto.sync.getLatestCommit", &format!("did={did}"))
        .await;
    assert_eq!(
        latest,
        json!({"cid": second.cid.to_string(), "rev": second.rev})
    );
    let signing = PublicKey::from_did_key(
        &h.host
            .store
            .account(&did)
            .unwrap()
            .unwrap()
            .signing
            .public()
            .did_key(),
    )
    .unwrap();
    assert!(commit.verify(&signing));
    let tree = Tree::read(commit.data, &car.blocks).unwrap();
    let keys: Vec<&str> = tree.leaves().into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, ["fyi.opensocial.rule/a", "fyi.opensocial.rule/b"]);
    assert!(
        tree.leaves()
            .iter()
            .all(|(_, cid)| car.blocks.contains_key(cid))
    );
}

#[tokio::test]
async fn refuses_writes_it_cannot_make() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    h.host.write(&did, &[create("a", "Ride safe")]).unwrap();
    let refused = |writes: &[Write]| h.host.write(&did, writes).unwrap_err();
    assert_eq!(
        h.host.write("did:plc:nobody", &[]).unwrap_err().error,
        "RepoNotFound"
    );
    let bad_path = Write::Delete {
        collection: "not an nsid".into(),
        rkey: "a".into(),
    };
    assert_eq!(
        refused(&[bad_path]).message,
        "not an nsid/a is not a record's path"
    );
    assert_eq!(
        refused(&[create("a", "again")]).message,
        "there is a record already at fyi.opensocial.rule/a"
    );
    let missing = Write::Delete {
        collection: "fyi.opensocial.rule".into(),
        rkey: "z".into(),
    };
    assert_eq!(
        refused(&[missing]).message,
        "there is no record at fyi.opensocial.rule/z"
    );
    let float = Write::Create {
        collection: "fyi.opensocial.rule".into(),
        rkey: "f".into(),
        value: json!({"$type": "fyi.opensocial.rule", "n": 1.5}),
    };
    assert_eq!(refused(&[float]).message, "1.5 is not an integer");
    let mistyped = Write::Create {
        collection: "fyi.opensocial.rule".into(),
        rkey: "t".into(),
        value: json!({"$type": "fyi.opensocial.role"}),
    };
    assert_eq!(
        refused(&[mistyped]).message,
        "a record in fyi.opensocial.rule has $type fyi.opensocial.rule"
    );
}

#[tokio::test]
async fn answers_what_is_not_here() {
    let h = start(&[], false).await;
    for (nsid, query, error) in [
        (
            "com.atproto.repo.getRecord",
            "collection=a.b.c&rkey=x",
            "InvalidRequest",
        ),
        (
            "com.atproto.repo.getRecord",
            "repo=nobody.groups.test&collection=a.b.c&rkey=x",
            "RepoNotFound",
        ),
        (
            "com.atproto.repo.describeRepo",
            "repo=did:plc:nobody",
            "RepoNotFound",
        ),
        (
            "com.atproto.sync.getRepo",
            "did=did:plc:nobody",
            "RepoNotFound",
        ),
        (
            "com.atproto.sync.getLatestCommit",
            "did=did:plc:nobody",
            "RepoNotFound",
        ),
        (
            "com.atproto.identity.resolveHandle",
            "handle=nobody.groups.test",
            "HandleNotFound",
        ),
    ] {
        let (status, body) = h.get(nsid, query).await;
        assert_eq!(
            (status, body["error"].as_str()),
            (400, Some(error)),
            "{nsid}?{query}"
        );
    }
    let (_, body) = h.get("com.atproto.repo.getRecord", "").await;
    assert_eq!(body["message"], "repo is required");
    let (_, body) = h.get("com.atproto.sync.listRepos", "limit=0").await;
    assert_eq!(body["message"], "limit is 1 to 100");
    let res = h
        .http
        .get(format!("{}/.well-known/atproto-did", h.url))
        .header("host", "nobody.groups.test")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn lists_repos_a_page_at_a_time() {
    let h = start(&[], false).await;
    let a = h.host.create_account("a.groups.test").await.unwrap();
    let b = h.host.create_account("b.groups.test").await.unwrap();
    let (first, second) = if a < b { (a, b) } else { (b, a) };
    let (_, page) = h.get("com.atproto.sync.listRepos", "limit=1").await;
    assert_eq!(page["repos"][0]["did"], first.as_str());
    assert_eq!(page["cursor"], first.as_str());
    let (_, page) = h
        .get(
            "com.atproto.sync.listRepos",
            &format!("limit=1&cursor={first}"),
        )
        .await;
    assert_eq!(page["repos"][0]["did"], second.as_str());
}

#[tokio::test]
async fn serves_its_own_did_document() {
    let h = start(&[], false).await;
    let doc: Value = h
        .http
        .get(format!("{}/.well-known/did.json", h.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(doc["id"], "did:web:host.test");
    assert_eq!(
        atproto::did::signing_key(&doc).unwrap(),
        h.host.secrets.signing.public()
    );
    assert_eq!(
        atproto::did::pds(&doc).as_deref(),
        Some("https://host.test")
    );
    assert_eq!(
        atproto::did::service(&doc, "#opensocial").as_deref(),
        Some("https://host.test")
    );

    let missing = h.host.plc.document("did:plc:nobody").await.unwrap_err();
    assert_eq!(missing.to_string(), "no document for did:plc:nobody");

    let plc = start(&[("SERVICE_DID", "did:plc:host")], false).await;
    let res = plc
        .http
        .get(format!("{}/.well-known/did.json", plc.url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn keeps_its_keys_and_data_across_starts() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let again = group_host::Host::open(h.host.config.clone()).unwrap();
    assert_eq!(
        again.secrets.rotation.public(),
        h.host.secrets.rotation.public()
    );
    assert_eq!(
        again.secrets.signing.public(),
        h.host.secrets.signing.public()
    );
    assert_eq!(again.store.account(RIDERS).unwrap().unwrap().did, did);

    let broken = |text: &str| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("host-secrets.json"), text).unwrap();
        group_host::secrets::Secrets::load(dir.path())
            .err()
            .unwrap()
            .to_string()
    };
    assert_eq!(broken("{"), "host-secrets.json is not JSON");
    assert_eq!(
        broken(r#"{"rotationKey": "abc"}"#),
        "host-secrets.json has no rotationKey"
    );
    assert_eq!(
        broken(r#"{"rotationKey": "zz"}"#),
        "host-secrets.json has no rotationKey"
    );
}

#[test]
fn a_commit_builds_on_the_head_it_read() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let account = Account {
        did: "did:plc:a".into(),
        handle: "a.groups.test".into(),
        signing: atproto::Keypair::generate(atproto::Curve::K256),
        kem: atproto::Keypair::generate(atproto::Curve::P256),
    };
    let tree = Tree::build(&Default::default());
    let commit = Commit::sign(&account.did, tree.root(), "3l5mz5mkbn22b", &account.signing);
    let head = Head {
        cid: commit.cid(),
        rev: commit.rev.clone(),
        block: commit.to_bytes(),
    };
    assert!(store.create_account(&account, &head, 0, &[]).unwrap());
    // Another account at the same handle is turned away.
    let twin = Account {
        did: "did:plc:b".into(),
        ..account
    };
    assert!(!store.create_account(&twin, &head, 0, &[]).unwrap());
    // A commit from a head that is no longer the head changes nothing.
    let stale = atproto::Cid::of_cbor(b"\xa0");
    assert!(
        store
            .commit("did:plc:a", &stale, &head, &[], &[])
            .unwrap()
            .is_none()
    );
    assert_eq!(store.head("did:plc:a").unwrap().unwrap(), head);
}

#[test]
fn says_what_went_wrong_inside() {
    let err = XrpcError::internal("the database is gone");
    assert_eq!(
        (err.status.as_u16(), err.error, err.message.as_str()),
        (500, "InternalServerError", "the database is gone")
    );
}

#[tokio::test]
async fn exports_a_tree_of_many_layers() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let writes: Vec<Write> = (0..200)
        .map(|i| create(&format!("r{i:03}"), "A rule"))
        .collect();
    h.host.write(&did, &writes).unwrap();
    let res = h
        .http
        .get(format!("{}/xrpc/com.atproto.sync.getRepo?did={did}", h.url))
        .send()
        .await
        .unwrap();
    let car = car::read(&res.bytes().await.unwrap()).unwrap();
    let commit = Commit::from_bytes(&car.blocks[&car.roots[0]]).unwrap();
    let tree = Tree::read(commit.data, &car.blocks).unwrap();
    assert_eq!(tree.leaves().len(), 200);
    assert!(tree.nodes().len() > 2);
}
