//! Groups' accounts and repos: made on the PLC directory, written a
//! signed commit at a time, and read back as a PDS answers them.

mod common;

use atproto::mst::Tree;
use atproto::{Cid, Commit, PublicKey, car, drisl};
use common::start;
use group_host::repo::Write;
use group_host::store::{Account, Head, Store};
use group_host::xrpc::XrpcError;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const RIDERS: &str = "riders.groups.test";

fn create(rkey: &str, text: &str) -> Write {
    Write::Create {
        collection: "org.atgrp.rule".into(),
        rkey: rkey.into(),
        value: json!({"$type": "org.atgrp.rule", "title": text, "createdAt": "2026-11-03T00:00:00Z"}),
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
                    collection: "org.atgrp.rule".into(),
                    rkey: "b".into(),
                    value: json!({"$type": "org.atgrp.rule", "title": "Be kinder", "createdAt": "2026-11-03T00:00:00Z"}),
                },
                Write::Delete { collection: "org.atgrp.rule".into(), rkey: "c".into() },
            ],
        )
        .unwrap();
    assert!(second.rev > first.rev);

    let (status, b) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={did}&collection=org.atgrp.rule&rkey=b"),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(b["value"]["title"], "Be kinder");
    assert_eq!(b["uri"], format!("at://{did}/org.atgrp.rule/b"));
    let cid = b["cid"].as_str().unwrap();
    let (status, _) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={RIDERS}&collection=org.atgrp.rule&rkey=b&cid={cid}"),
        )
        .await;
    assert_eq!(status, 200);
    let (status, gone) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={did}&collection=org.atgrp.rule&rkey=c"),
        )
        .await;
    assert_eq!(
        (status, gone["error"].as_str()),
        (400, Some("RecordNotFound"))
    );
    let (status, _) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={did}&collection=org.atgrp.rule&rkey=a&cid={cid}"),
        )
        .await;
    assert_eq!(status, 400);

    // Newest key first, a page at a time; oldest first with reverse.
    let lists = "com.atproto.repo.listRecords";
    let list = |q: &str| format!("repo={did}&collection=org.atgrp.rule&{q}");
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
    assert_eq!(repo["collections"], json!(["org.atgrp.rule"]));

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
    assert_eq!(keys, ["org.atgrp.rule/a", "org.atgrp.rule/b"]);
    assert!(
        tree.leaves()
            .iter()
            .all(|(_, cid)| car.blocks.contains_key(cid))
    );
}

#[tokio::test]
async fn makes_a_groups_writes_one_at_a_time_and_others_beside_them() {
    let h = start(&[], false).await;
    let riders = h.host.create_account(RIDERS).await.unwrap();
    let walkers = h.host.create_account("walkers.groups.test").await.unwrap();
    // Each group written from many threads at once, riders by its DID and
    // by its handle alike: every write is made, none refused for a head
    // another moved under it.
    let names = [riders.as_str(), RIDERS, walkers.as_str()];
    std::thread::scope(|s| {
        for (n, name) in names.iter().enumerate() {
            for t in 0..8 {
                let host = &h.host;
                s.spawn(move || {
                    for i in 0..5 {
                        let rkey = format!("n{n}t{t}i{i}");
                        host.write(name, &[create(&rkey, "Ride safe")]).unwrap();
                    }
                });
            }
        }
    });
    assert_eq!(h.host.store.paths(&riders).unwrap().len(), 80);
    assert_eq!(h.host.store.paths(&walkers).unwrap().len(), 40);
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
        "there is a record already at org.atgrp.rule/a"
    );
    let missing = Write::Delete {
        collection: "org.atgrp.rule".into(),
        rkey: "z".into(),
    };
    assert_eq!(
        refused(&[missing]).message,
        "there is no record at org.atgrp.rule/z"
    );
    let float = Write::Create {
        collection: "org.atgrp.rule".into(),
        rkey: "f".into(),
        value: json!({"$type": "org.atgrp.rule", "n": 1.5}),
    };
    assert_eq!(refused(&[float]).message, "1.5 is not an integer");
    let mistyped = Write::Create {
        collection: "org.atgrp.rule".into(),
        rkey: "t".into(),
        value: json!({"$type": "org.atgrp.role"}),
    };
    assert_eq!(
        refused(&[mistyped]).message,
        "a record in org.atgrp.rule has $type org.atgrp.rule"
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
    // Its pages are atproto's: up to 1000, as a relay asks.
    let (_, body) = h.get("com.atproto.sync.listRepos", "limit=1001").await;
    assert_eq!(body["message"], "limit is 1 to 1000");
    let (status, _) = h.get("com.atproto.sync.listRepos", "limit=1000").await;
    assert_eq!(status, 200);
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
        atproto::did::service(&doc, "#atgrp").as_deref(),
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

#[tokio::test]
async fn describes_itself_as_a_pds_that_names_groups_under_its_domain() {
    let h = start(&[("HANDLE_DOMAIN", ".groups.test")], false).await;
    let (status, described) = h.get("com.atproto.server.describeServer", "").await;
    assert_eq!(status, 200, "{described}");
    assert_eq!(described["did"], h.host.config.service_did.as_str());
    assert_eq!(
        described["availableUserDomains"],
        serde_json::json!([".groups.test"])
    );
    // Nobody signs up here: a group is made for someone.
    assert_eq!(described["inviteCodeRequired"], true);
    assert_eq!(
        described["blobUploadLimit"],
        h.host.config.blob_upload_limit
    );
}

/// The record `key` names in the tree under `root`, found down the path
/// `blocks` prove: `None` when the path proves it absent. A node the path
/// needs and `blocks` lack fails it, as an app reading the proof fails.
fn proved(blocks: &BTreeMap<Cid, Vec<u8>>, root: Cid, key: &str) -> Option<Cid> {
    let link = |v: &drisl::Value| match v {
        drisl::Value::Link(c) => Some(*c),
        _ => None,
    };
    let mut at = root;
    loop {
        let drisl::Value::Map(node) = drisl::decode(&blocks[&at]).unwrap() else {
            panic!("{at} is not a tree node");
        };
        let drisl::Value::Array(written) = &node["e"] else {
            panic!("a node without entries");
        };
        let (mut last, mut below) = (Vec::new(), link(&node["l"]));
        for entry in written {
            let drisl::Value::Map(e) = entry else {
                panic!("an entry that is not a map");
            };
            let (drisl::Value::Integer(shared), drisl::Value::Bytes(rest)) = (&e["p"], &e["k"])
            else {
                panic!("an entry without its key");
            };
            last.truncate(*shared as usize);
            last.extend_from_slice(rest);
            let here = std::str::from_utf8(&last).unwrap();
            if here == key {
                return link(&e["v"]);
            }
            if here > key {
                break;
            }
            below = link(&e["t"]);
        }
        at = below?;
    }
}

#[tokio::test]
async fn proves_a_record_or_its_absence_from_the_signed_commit() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let writes: Vec<Write> = (0..200)
        .map(|i| create(&format!("r{i:03}"), "A rule"))
        .collect();
    let head = h.host.write(&did, &writes).unwrap();
    let signing = h.host.store.account(&did).unwrap().unwrap().signing;
    let signing = PublicKey::from_did_key(&signing.public().did_key()).unwrap();
    let proof = |rkey: &str| {
        let url = format!(
            "{}/xrpc/com.atproto.sync.getRecord?did={did}&collection=org.atgrp.rule&rkey={rkey}",
            h.url
        );
        let http = h.http.clone();
        async move {
            let res = http.get(url).send().await.unwrap();
            assert_eq!(res.status(), 200);
            assert_eq!(res.headers()["content-type"], "application/vnd.ipld.car");
            car::read(&res.bytes().await.unwrap()).unwrap()
        }
    };

    // The record, reached from the signed commit's tree, its block with it.
    let car = proof("r123").await;
    assert_eq!(car.roots, [head.cid]);
    let commit = Commit::verified(&car.blocks[&car.roots[0]], &did, &signing).unwrap();
    let cid = proved(&car.blocks, commit.data, "org.atgrp.rule/r123").unwrap();
    let (_, record) = h
        .get(
            "com.atproto.repo.getRecord",
            &format!("repo={did}&collection=org.atgrp.rule&rkey=r123"),
        )
        .await;
    assert_eq!(cid.to_string(), record["cid"]);
    assert_eq!(Cid::of_cbor(&car.blocks[&cid]), cid);
    // A proof, not the repo: a path down a tree of many layers.
    assert!(car.blocks.len() < 20);

    // A record not there is proved absent from the same commit.
    for rkey in ["r123x", "a", "zzz"] {
        let car = proof(rkey).await;
        let commit = Commit::verified(&car.blocks[&car.roots[0]], &did, &signing).unwrap();
        let key = format!("org.atgrp.rule/{rkey}");
        assert_eq!(proved(&car.blocks, commit.data, &key), None, "{rkey}");
    }

    // A repo not here, and a group switched off, are refused by name.
    for (query, error) in [
        (
            "did=did:plc:nobody&collection=org.atgrp.rule&rkey=r1",
            "RepoNotFound",
        ),
        (&*format!("did={did}&rkey=r1"), "InvalidRequest"),
    ] {
        let (status, body) = h.get("com.atproto.sync.getRecord", query).await;
        assert_eq!((status, body["error"].as_str()), (400, Some(error)));
    }
    h.host.deactivate_account(&did, &did, &[]).unwrap();
    let (status, body) = h
        .get(
            "com.atproto.sync.getRecord",
            &format!("did={did}&collection=org.atgrp.rule&rkey=r123"),
        )
        .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("RepoDeactivated"))
    );
}
