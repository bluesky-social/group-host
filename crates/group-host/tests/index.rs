//! The host's index of its groups' spaces: read from each writer's repo,
//! here or at their PDS, checked against what their host vouches for, and
//! served to whoever the space lets read.

mod common;

use atproto::plc::Op;
use atproto::space::{
    COMMIT_MAC, Commit, LtHash, decapsulate, element, mac_headers, read_accept_signature, star,
};
use atproto::{Cid, Curve, Keypair, Value, drisl};
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get as route_get;
use common::groups::*;
use common::oauth::make_space;
use common::spaces::*;
use common::{Running, serve, start, start_reachable};
use group_host::group::members_space;
use group_host::repo::Write;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const NS: &str = "org.atgrp";
const NOTE: &str = "org.atgrp.note";

fn note(text: &str) -> Json {
    json!({"$type": NOTE, "text": text, "createdAt": "2026-11-03T00:00:00Z"})
}

fn create(rkey: &str, value: Json) -> Write {
    Write::Create {
        collection: NOTE.into(),
        rkey: rkey.into(),
        value,
    }
}

/// A group Alice started on `h`, with a space any member writes in and
/// reads, and `member` a member of it.
async fn group_with(h: &Running, founder: &Authority, member: &str) -> (String, String) {
    let did = start_group(
        h,
        founder,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let policy = json!({"$type": "org.atgrp.policy", "grants": [{"role": "member", "read": true, "write": true}]});
    let made = make_space(h, founder, &did, "org.atgrp.board", "notes", policy).await;
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[Write::Create {
                collection: format!("{NS}.membership"),
                rkey: member.into(),
                value: json!({"$type": format!("{NS}.membership"), "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    (did, made)
}

async fn list(h: &Running, who: Option<&Authority>, query: &str) -> (u16, Json) {
    let lxm = format!("{INTERNAL}.listSpaceRecords");
    let headers: Vec<_> = who.map(|w| auth(h, w, &lxm)).into_iter().collect();
    get(h, &lxm, query, &headers).await
}

fn texts(page: &Json) -> Vec<String> {
    page["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["value"]["text"].as_str().map(String::from))
        .collect()
}

#[tokio::test]
async fn indexes_a_member_writing_at_another_host() {
    // The group's host, and Bob's, on one directory.
    let a = start_reachable(&[]).await;
    let plc = a.host.config.plc_url.clone();
    let b = start_reachable(&[("PLC_URL", plc.as_str())]).await;
    let bob = b.host.create_account("bob.groups.test").await.unwrap();
    let [alice, _, carol] = people(&a).await;
    let (did, space) = group_with(&a, &alice, &bob).await;

    // Bob writes at his own host, which tells the group's; the index reads
    // his repo whole the first time.
    b.host
        .space_write(
            &bob,
            &space,
            &[create("one", note("First")), create("two", note("Second"))],
        )
        .await
        .unwrap();
    a.host.index_pending().await;
    let query = format!("group={did}&space={space}");
    let (status, page) = list(&a, Some(&alice), &query).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(texts(&page), ["Second", "First"]);
    assert_eq!(page["records"][0]["repo"], bob);
    assert_eq!(
        page["records"][0]["uri"],
        format!("{space}/{bob}/{NOTE}/two")
    );

    // And reads on from there as he writes again.
    let picture = b.host.upload_blob(&bob, b"a map", "image/png").unwrap();
    b.host
        .space_write(
            &bob,
            &space,
            &[
                Write::Update {
                    collection: NOTE.into(),
                    rkey: "one".into(),
                    value: note("First, again"),
                },
                Write::Delete {
                    collection: NOTE.into(),
                    rkey: "two".into(),
                },
                create("three", json!({"$type": NOTE, "text": "Third", "map": picture, "subject": {"uri": "at://x"}})),
            ],
        )
        .await
        .unwrap();
    a.host.index_pending().await;
    let (_, page) = list(&a, Some(&alice), &query).await;
    assert_eq!(texts(&page), ["Third", "First, again"]);

    // Its files come from his host, through the group's.
    let lxm = format!("{INTERNAL}.getSpaceBlob");
    let cid = picture["ref"]["$link"].as_str().unwrap();
    let got = read(
        &a,
        &lxm,
        &format!("{query}&repo={bob}&cid={cid}"),
        &[auth(&a, &alice, &lxm)],
    )
    .await;
    assert_eq!((got.status, got.body.as_slice()), (200, &b"a map"[..]));
    assert_eq!(got.headers["content-type"], "image/png");
    assert_eq!(got.headers["x-content-type-options"], "nosniff");
    // One not in the space, or not his, is not found.
    let other = b.host.upload_blob(&bob, b"elsewhere", "image/png").unwrap();
    let other = other["ref"]["$link"].as_str().unwrap();
    let got = read(
        &a,
        &lxm,
        &format!("{query}&repo={bob}&cid={other}"),
        &[auth(&a, &alice, &lxm)],
    )
    .await;
    assert_eq!(got.error(), (400, "NotFound".into()));
    let got = read(
        &a,
        &lxm,
        &format!("{query}&repo={bob}&cid=bafynotacid"),
        &[auth(&a, &alice, &lxm)],
    )
    .await;
    assert_eq!(got.status, 400);
    // The group's own files in the space, from the store.
    let ours = a.host.upload_blob(&did, b"ours", "image/png").unwrap();
    a.host
        .space_write(
            &did,
            &space,
            &[create(
                "pinned",
                json!({"$type": NOTE, "text": "Pinned", "map": ours}),
            )],
        )
        .await
        .unwrap();
    let ours = ours["ref"]["$link"].as_str().unwrap();
    let got = read(
        &a,
        &lxm,
        &format!("{query}&repo={did}&cid={ours}"),
        &[auth(&a, &alice, &lxm)],
    )
    .await;
    assert_eq!(got.body, b"ours");
    let got = read(
        &a,
        &lxm,
        &format!("{query}&repo={did}&cid={other}"),
        &[auth(&a, &alice, &lxm)],
    )
    .await;
    assert_eq!(got.status, 400);

    // Only for those the space lets read.
    let (status, said) = list(&a, Some(&carol), &query).await;
    assert_eq!(
        (status, said["error"].as_str().unwrap()),
        (401, "NotAuthorized")
    );
    let (status, _) = list(&a, None, &query).await;
    assert_eq!(status, 401);
    let got = read(
        &a,
        &lxm,
        &format!("{query}&repo={bob}&cid={cid}"),
        &[auth(&a, &carol, &lxm)],
    )
    .await;
    assert_eq!(got.status, 401);
}

#[tokio::test]
async fn a_public_space_is_for_anyone_signed_in() {
    let h = start(&[], false).await;
    let [alice, _, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let policy = json!({"$type": "org.atgrp.policy", "publicRead": true, "grants": []});
    let space = &make_space(&h, &alice, &did, "org.atgrp.board", "open", policy).await;
    let picture = h.host.upload_blob(&did, b"a map", "image/png").unwrap();
    h.host
        .space_write(
            &did,
            space,
            &[create(
                "one",
                json!({"$type": NOTE, "text": "Open", "map": picture}),
            )],
        )
        .await
        .unwrap();
    let query = format!("group={did}&space={space}");
    // A stranger signed in reads it.
    let (status, page) = list(&h, Some(&carol), &query).await;
    assert_eq!((status, texts(&page)), (200, vec!["Open".to_string()]));
    // No one signed in reads it, nor its files.
    let (status, said) = list(&h, None, &query).await;
    assert_eq!(
        (status, said["error"].as_str().unwrap()),
        (401, "AuthMissing")
    );
    let cid = picture["ref"]["$link"].as_str().unwrap();
    let blob = format!("{INTERNAL}.getSpaceBlob");
    let got = read(&h, &blob, &format!("{query}&repo={did}&cid={cid}"), &[]).await;
    assert_eq!(got.error(), (401, "AuthMissing".into()));
}

#[tokio::test]
async fn pages_filters_and_finds_one() {
    let h = start(&[], false).await;
    let [alice, _, _] = people(&h).await;
    let (did, space) = group_with(&h, &alice, "did:plc:nobody").await;
    let writes: Vec<Write> = (0..5)
        .map(|i| create(&format!("n{i}"), json!({"$type": NOTE, "text": format!("Note {i}"), "subject": {"uri": format!("at://s/{}", i % 2)}})))
        .chain([Write::Create {
            collection: "org.atgrp.other".into(),
            rkey: "x".into(),
            value: json!({"$type": "org.atgrp.other", "text": "Other"}),
        }])
        .collect();
    h.host.space_write(&did, &space, &writes).await.unwrap();
    let query = format!("group={did}&space={space}");
    // The group's own repo, from the store, read as the list is asked:
    // its published policy there among them.
    let (_, all) = list(&h, Some(&alice), &query).await;
    assert_eq!(all["records"].as_array().unwrap().len(), 7);

    let (_, notes) = list(
        &h,
        Some(&alice),
        &format!("{query}&collection={NOTE}&limit=2"),
    )
    .await;
    assert_eq!(notes["records"].as_array().unwrap().len(), 2);
    let cursor = notes["cursor"].as_str().unwrap();
    let (_, more) = list(
        &h,
        Some(&alice),
        &format!("{query}&collection={NOTE}&limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(more["records"].as_array().unwrap().len(), 2);
    assert_ne!(texts(&more), texts(&notes));

    // Those whose field, by a dotted path, equals what is asked: a page
    // read on through the index until it is full.
    let (_, odd) = list(
        &h,
        Some(&alice),
        &format!("{query}&field=subject.uri&equals=at://s/1&limit=1"),
    )
    .await;
    assert_eq!(texts(&odd).len(), 1);
    assert!(odd["cursor"].is_string());
    let (_, none) = list(
        &h,
        Some(&alice),
        &format!("{query}&field=subject.uri&equals=at://s/9"),
    )
    .await;
    assert_eq!(none, json!({"records": [], "cursor": null}));
    let (_, numbered) = list(&h, Some(&alice), &format!("{query}&field=subject&equals=x")).await;
    assert_eq!(numbered["records"], json!([]));

    // One, by its URI.
    let uri = format!("{space}/{did}/{NOTE}/n3");
    let (_, one) = list(&h, Some(&alice), &format!("{query}&uri={uri}")).await;
    assert_eq!(texts(&one), ["Note 3"]);
    let (_, missing) = list(
        &h,
        Some(&alice),
        &format!("{query}&uri={space}/{did}/{NOTE}/n9"),
    )
    .await;
    assert_eq!(missing["records"], json!([]));
    for (rest, refused) in [
        ("&uri=at://elsewhere.example.com", (400, "NotFound")),
        ("&cursor=x", (400, "InvalidRequest")),
        ("&limit=101", (400, "InvalidRequest")),
    ] {
        let said = list(&h, Some(&alice), &format!("{query}{rest}")).await;
        assert_eq!(
            (said.0, said.1["error"].as_str().unwrap()),
            refused,
            "{rest}"
        );
    }
    // A space not the group's, or none, or a group not here or off.
    for query in [
        format!("group={did}&space=at:space/did:plc:other/{NS}.board/notes"),
        format!("group={did}&space=nope"),
        format!("group={did}&space=at:space/{did}/{NS}.board/none"),
        format!("group={did}&space=at://{did}/space/{NS}.board/notes"),
        format!("group=did:plc:nobody&space={space}"),
    ] {
        let said = list(&h, Some(&alice), &query).await;
        assert_eq!(
            (said.0, said.1["error"].as_str().unwrap()),
            (400, "NotFound"),
            "{query}"
        );
    }
    h.host.deactivate_account(&did, &alice.did, &[]).unwrap();
    let said = list(&h, Some(&alice), &query).await;
    assert_eq!(said.1["error"], "NotFound");
}

/// A writer's PDS that answers as a test tells it: each method's answer
/// made from the secret the reader agreed, and how often it was asked.
#[derive(Default)]
struct Fake {
    kem: Mutex<Option<Keypair>>,
    asked: Mutex<usize>,
    answers: Mutex<BTreeMap<String, Vec<Answer>>>,
}

/// One answer: its status, body, and the commit it vouches for, if any,
/// with a MAC right or wrong.
#[derive(Clone)]
struct Answer {
    status: u16,
    body: Vec<u8>,
    vouch: Option<(Commit, bool)>,
}

async fn answer(
    State(fake): State<Arc<Fake>>,
    Path(nsid): Path<String>,
    headers: HeaderMap,
) -> Response {
    let method = nsid.trim_start_matches("com.atproto.space.").to_string();
    *fake.asked.lock().unwrap() += 1;
    let next = {
        let mut answers = fake.answers.lock().unwrap();
        let queue = answers.entry(method).or_default();
        if queue.len() > 1 {
            Some(queue.remove(0))
        } else {
            queue.first().cloned()
        }
    };
    let Some(next) = next else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut res = (StatusCode::from_u16(next.status).unwrap(), next.body).into_response();
    if let Some((commit, right)) = next.vouch {
        let asked = headers["accept-signature"].to_str().unwrap();
        let request = read_accept_signature(asked, COMMIT_MAC).unwrap().unwrap();
        let kem = fake.kem.lock().unwrap().clone().unwrap();
        let secret = decapsulate(&kem, &request.key_share).unwrap();
        let secret = if right { secret } else { [0; 32] };
        for (name, value) in mac_headers(&request, COMMIT_MAC, &commit.header(), &secret) {
            let name = axum::http::HeaderName::try_from(name).unwrap();
            res.headers_mut().insert(name, value.parse().unwrap());
        }
    }
    res
}

/// A writer on the directory whose PDS is `fake`: its DID.
async fn fake_writer(h: &Running, fake: &Arc<Fake>) -> String {
    let router = Router::new()
        .route("/xrpc/{nsid}", route_get(answer))
        .with_state(fake.clone());
    let url = serve(router).await;
    let kem = Keypair::generate(Curve::P256);
    let rotation = Keypair::generate(Curve::K256);
    let mut op = Op::atproto(
        &Keypair::generate(Curve::K256).public().did_key(),
        &[rotation.public().did_key()],
        "fake.test",
        &url,
    );
    op.verification_methods
        .insert("atproto_kem".into(), kem.public().did_key());
    *fake.kem.lock().unwrap() = Some(kem);
    let genesis = op.sign(None, &rotation);
    let did = genesis.did();
    h.http
        .post(format!("{}/{did}", h.host.config.plc_url))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    did
}

fn commit(writer: &str, space: &str, rev: &str, hash: &LtHash) -> Commit {
    Commit {
        repo: writer.into(),
        space: space.into(),
        rev: rev.into(),
        data: hash.digest(),
    }
}

fn commit_json(c: &Commit) -> Json {
    json!({"ver": 1, "repo": c.repo, "space": c.space, "rev": c.rev, "data": Value::Bytes(c.data.to_vec()).to_json()})
}

fn bytes_of(value: &Json) -> Vec<u8> {
    drisl::encode(&Value::from_json(value).unwrap())
}

#[tokio::test]
async fn takes_only_what_a_writers_host_vouches_for() {
    // On http, as on a network of test servers: the PDS is on this machine.
    let h = start(&[("PUBLIC_URL", "http://host.test")], false).await;
    let fake = Arc::new(Fake::default());
    let writer = fake_writer(&h, &fake).await;
    let [alice, _, _] = people(&h).await;
    let (did, space) = group_with(&h, &alice, &writer).await;
    let query = format!("group={did}&space={space}");
    let indexed = |h: &Running| h.host.store.index_state(&space, &writer).unwrap();

    // Whole: a record that is no map is hashed, and left out.
    let first = bytes_of(&note("First"));
    let odd = drisl::encode(&Value::Integer(1));
    let mut hash = LtHash::default();
    hash.add(&element(NOTE, "one", &Cid::of_cbor(&first)));
    hash.add(&element(NOTE, "odd", &Cid::of_cbor(&odd)));
    let at_one = commit(&writer, &space, "3l5mz5mkbn22b", &hash);
    let records = BTreeMap::from([
        (format!("{NOTE}/one"), first.clone()),
        (format!("{NOTE}/odd"), odd),
    ]);
    let whole = star(&at_one, &records);
    let tell = |c: &Commit, answers: Vec<(&str, Answer)>| {
        let mut queued = fake.answers.lock().unwrap();
        queued.clear();
        for (method, a) in answers {
            queued.entry(method.into()).or_default().push(a);
        }
        h.host
            .store
            .record_writer(&space, &writer, &c.rev, &c.data, &h.host.clock.next())
            .unwrap();
    };
    let ok = |body: Vec<u8>, c: &Commit| Answer {
        status: 200,
        body,
        vouch: Some((c.clone(), true)),
    };

    // Refused: not vouched for, another's, not what it says, not an
    // archive, not answered.
    let elsewhere = Commit {
        repo: "did:plc:other".into(),
        ..at_one.clone()
    };
    let wrong = Commit {
        data: [9; 32],
        ..at_one.clone()
    };
    for refused in [
        Answer {
            status: 200,
            body: whole.clone(),
            vouch: Some((at_one.clone(), false)),
        },
        Answer {
            status: 200,
            body: star(&elsewhere, &records),
            vouch: Some((elsewhere.clone(), true)),
        },
        Answer {
            status: 200,
            body: star(&wrong, &records),
            vouch: Some((wrong.clone(), true)),
        },
        Answer {
            status: 200,
            body: b"nope".to_vec(),
            vouch: None,
        },
        Answer {
            status: 500,
            body: vec![],
            vouch: None,
        },
    ] {
        tell(&at_one, vec![("getRepo", refused)]);
        h.host.sync_space(&space).await.unwrap();
        assert_eq!(indexed(&h), None);
    }
    tell(&at_one, vec![("getRepo", ok(whole.clone(), &at_one))]);
    h.host.sync_space(&space).await.unwrap();
    assert_eq!(indexed(&h).unwrap().rev, "3l5mz5mkbn22b");
    let (_, page) = list(&h, Some(&alice), &query).await;
    assert_eq!(texts(&page), ["First"]);
    // Read already, it is not read again.
    h.host.sync_space(&space).await.unwrap();

    // Read on: two pages, the commit vouched for on the last.
    let second = bytes_of(&note("Second"));
    let mut on = hash.clone();
    on.add(&element(NOTE, "two", &Cid::of_cbor(&second)));
    on.remove(&element(NOTE, "one", &Cid::of_cbor(&first)));
    let at_two = commit(&writer, &space, "3l5mz5mkbn23b", &on);
    let page_one = json!({"ops": [{"rev": "3l5mz5mkbn23b", "collection": NOTE, "rkey": "two", "cid": Cid::of_cbor(&second).to_string(), "prev": null, "value": note("Second")}], "cursor": "3l5mz5mkbn23b/0"});
    let page_two = json!({"ops": [{"rev": "3l5mz5mkbn23b", "collection": NOTE, "rkey": "one", "cid": null, "prev": Cid::of_cbor(&first).to_string()}], "commit": commit_json(&at_two)});
    let pages = |two: Json, vouch: Option<(Commit, bool)>| {
        vec![
            (
                "listRepoOps",
                Answer {
                    status: 200,
                    body: page_one.to_string().into_bytes(),
                    vouch: None,
                },
            ),
            (
                "listRepoOps",
                Answer {
                    status: 200,
                    body: two.to_string().into_bytes(),
                    vouch,
                },
            ),
        ]
    };
    tell(
        &at_two,
        pages(page_two.clone(), Some((at_two.clone(), true))),
    );
    h.host.sync_space(&space).await.unwrap();
    assert_eq!(indexed(&h).unwrap().rev, "3l5mz5mkbn23b");
    let (_, page) = list(&h, Some(&alice), &query).await;
    assert_eq!(texts(&page), ["Second"]);

    // An oplog that does not check out is read whole instead; here the
    // whole is refused too, so the index stays where it was.
    let mut at_three = at_two.clone();
    at_three.rev = "3l5mz5mkbn24b".into();
    at_three.data = [3; 32];
    let set = |mut p: Json, f: &dyn Fn(&mut Json)| {
        f(&mut p);
        p
    };
    let refused_whole = (
        "getRepo",
        Answer {
            status: 500,
            body: vec![],
            vouch: None,
        },
    );
    let cases: Vec<Vec<(&str, Answer)>> = vec![
        // Not vouched for, or of another repo.
        pages(
            set(page_two.clone(), &|p| p["commit"] = commit_json(&at_three)),
            Some((at_three.clone(), false)),
        ),
        pages(
            set(page_two.clone(), &|p| {
                p["commit"]["repo"] = json!("did:plc:other")
            }),
            Some((
                Commit {
                    repo: "did:plc:other".into(),
                    ..at_three.clone()
                },
                true,
            )),
        ),
        // Not where its commit says.
        pages(
            set(page_two.clone(), &|p| p["commit"] = commit_json(&at_three)),
            Some((at_three.clone(), true)),
        ),
        // A commit in the page that is not the one vouched for.
        pages(page_two.clone(), Some((at_three.clone(), true))),
        // A head with no commit, or one that is not one.
        pages(
            set(page_two.clone(), &|p| {
                p.as_object_mut().unwrap().remove("commit");
            }),
            Some((at_two.clone(), true)),
        ),
        pages(
            set(page_two.clone(), &|p| p["commit"]["ver"] = json!(2)),
            Some((at_two.clone(), true)),
        ),
        pages(
            set(page_two.clone(), &|p| p["commit"]["data"] = json!("x")),
            Some((at_two.clone(), true)),
        ),
        pages(
            set(page_two.clone(), &|p| {
                p["commit"]["data"] = Value::Bytes(vec![1]).to_json()
            }),
            Some((at_two.clone(), true)),
        ),
        pages(
            set(page_two.clone(), &|p| {
                p["commit"].as_object_mut().unwrap().remove("rev");
            }),
            Some((at_two.clone(), true)),
        ),
        // Ops that are not ops.
        pages(json!({"cursor": null}), None),
        pages(json!({"ops": [{"rkey": "x"}]}), None),
        pages(json!({"ops": [{"collection": NOTE}]}), None),
        pages(
            json!({"ops": [{"collection": NOTE, "rkey": "x", "prev": "nope"}]}),
            None,
        ),
        // A page given twice.
        vec![
            (
                "listRepoOps",
                Answer {
                    status: 200,
                    body: page_one.to_string().into_bytes(),
                    vouch: None,
                },
            ),
            (
                "listRepoOps",
                Answer {
                    status: 200,
                    body: page_one.to_string().into_bytes(),
                    vouch: None,
                },
            ),
        ],
        // Not an answer.
        vec![(
            "listRepoOps",
            Answer {
                status: 500,
                body: vec![],
                vouch: None,
            },
        )],
    ];
    for mut answers in cases {
        answers.push(refused_whole.clone());
        tell(&at_three, answers);
        h.host.sync_space(&space).await.unwrap();
        assert_eq!(indexed(&h).unwrap().rev, "3l5mz5mkbn23b");
    }
    // A write whose record is gone, or another, by the time it is read:
    // the repo is read whole, and that checks out.
    let third = bytes_of(&note("Third"));
    let mut thrice = on.clone();
    thrice.add(&element(NOTE, "three", &Cid::of_cbor(&third)));
    let at_three = commit(&writer, &space, "3l5mz5mkbn24b", &thrice);
    let op = |value: Json| {
        let mut op = json!({"rev": "3l5mz5mkbn24b", "collection": NOTE, "rkey": "three", "cid": Cid::of_cbor(&third).to_string(), "prev": null});
        if !value.is_null() {
            op["value"] = value;
        }
        json!({"ops": [op], "commit": commit_json(&at_three)})
    };
    let now_whole = BTreeMap::from([
        (format!("{NOTE}/two"), second.clone()),
        (format!("{NOTE}/three"), third.clone()),
        (format!("{NOTE}/odd"), drisl::encode(&Value::Integer(1))),
    ]);
    for stale in [Json::Null, note("Not third"), json!(1)] {
        tell(
            &at_three,
            vec![
                (
                    "listRepoOps",
                    Answer {
                        status: 200,
                        body: op(stale).to_string().into_bytes(),
                        vouch: Some((at_three.clone(), true)),
                    },
                ),
                ("getRepo", ok(star(&at_three, &now_whole), &at_three)),
            ],
        );
        h.host.sync_space(&space).await.unwrap();
        assert_eq!(indexed(&h).unwrap().rev, "3l5mz5mkbn24b");
        // Back, for the next.
        h.host
            .store
            .index(
                &space,
                &writer,
                &group_host::store::SpaceHead {
                    rev: "3l5mz5mkbn23b".into(),
                    state: on.state(),
                },
                true,
                &BTreeMap::new(),
                0,
            )
            .unwrap();
    }

    // A writer whose host cannot be found, or names no key, is left as it
    // was.
    let ghost = "did:plc:ghost";
    h.host
        .store
        .record_writer(
            &space,
            ghost,
            "3l5mz5mkbn22b",
            &[1; 32],
            &h.host.clock.next(),
        )
        .unwrap();
    h.host.sync_space(&space).await.unwrap();
    assert_eq!(
        h.host.store.index_state(&space, "did:plc:ghost").unwrap(),
        None
    );
    // A space with no authority here is nothing to index.
    h.host
        .sync_space("at:space/did:plc:other/x.y.z/self")
        .await
        .unwrap();
}

#[tokio::test]
async fn fetches_no_file_too_big_or_not_asked_for() {
    // On http, as on a network of test servers: the PDS is on this machine.
    let h = start(&[("PUBLIC_URL", "http://host.test")], false).await;
    let fake = Arc::new(Fake::default());
    let writer = fake_writer(&h, &fake).await;
    let [alice, _, _] = people(&h).await;
    let (did, space) = group_with(&h, &alice, &writer).await;
    let lxm = format!("{INTERNAL}.getSpaceBlob");
    let fetch = |cid: &str| format!("group={did}&space={space}&repo={writer}&cid={cid}");
    // Not a writer of the space yet: nobody is asked for its files.
    let got = read(
        &h,
        &lxm,
        &fetch(&Cid::of_raw(b"small").to_string()),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(got.error(), (400, "NotFound".into()));
    assert_eq!(*fake.asked.lock().unwrap(), 0);
    h.host
        .store
        .record_writer(
            &space,
            &writer,
            "3l5mz5mkbn22b",
            &[1; 32],
            &h.host.clock.next(),
        )
        .unwrap();
    let small = b"small".to_vec();
    let big = vec![0u8; 5 * 1024 * 1024 + 1];
    for (bytes, asked, ok) in [
        (small.clone(), Cid::of_raw(&small), true),
        (small.clone(), Cid::of_raw(b"other"), false),
        (big.clone(), Cid::of_raw(&big), false),
    ] {
        fake.answers.lock().unwrap().insert(
            "getBlob".into(),
            vec![Answer {
                status: 200,
                body: bytes,
                vouch: None,
            }],
        );
        let got = read(
            &h,
            &lxm,
            &fetch(&asked.to_string()),
            &[auth(&h, &alice, &lxm)],
        )
        .await;
        assert_eq!(got.status == 200, ok);
    }
    // One no longer a member, though still in the writer set: their files
    // are fetched no more.
    fake.answers.lock().unwrap().insert(
        "getBlob".into(),
        vec![Answer {
            status: 200,
            body: small.clone(),
            vouch: None,
        }],
    );
    let membership = (format!("{NS}.membership"), writer.to_string());
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[Write::Delete {
                collection: membership.0,
                rkey: membership.1,
            }],
        )
        .await
        .unwrap();
    let asked = *fake.asked.lock().unwrap();
    let got = read(
        &h,
        &lxm,
        &fetch(&Cid::of_raw(&small).to_string()),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(got.error(), (400, "NotFound".into()));
    assert_eq!(*fake.asked.lock().unwrap(), asked);
    // A writer whose host is not found.
    h.host
        .store
        .record_writer(
            &space,
            "did:plc:ghost",
            "3l5mz5mkbn22b",
            &[1; 32],
            &h.host.clock.next(),
        )
        .unwrap();
    let fetch = format!(
        "group={did}&space={space}&repo=did:plc:ghost&cid={}",
        Cid::of_raw(&small)
    );
    let got = read(&h, &lxm, &fetch, &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(got.status, 400);
}

#[tokio::test]
async fn keeps_a_record_read_again_where_it_was() {
    use group_host::store::SpaceHead;
    let h = start(&[], false).await;
    let [alice, _, _] = people(&h).await;
    let (_, space) = group_with(&h, &alice, "did:plc:writer").await;
    let store = &h.host.store;
    let put = |v: &str| {
        let value = v.as_bytes().to_vec();
        Some((Cid::of_raw(&value), value))
    };
    let head = |rev: &str| SpaceHead {
        rev: rev.into(),
        state: vec![0; 2048],
    };
    let first = BTreeMap::from([
        ("a.b.c/one".to_string(), put("one")),
        ("a.b.c/two".to_string(), put("two")),
    ]);
    store
        .index(&space, "did:plc:writer", &head("3l1"), true, &first, 100)
        .unwrap();
    // Read whole again: one unchanged, one changed, one new, none gone
    // but what is not there.
    let second = BTreeMap::from([
        ("a.b.c/one".to_string(), put("one")),
        ("a.b.c/two".to_string(), put("two, edited")),
        ("a.b.c/three".to_string(), put("three")),
    ]);
    store
        .index(&space, "did:plc:writer", &head("3l2"), true, &second, 200)
        .unwrap();
    let listed = store.indexed(&space, None, None, 10).unwrap();
    let order: Vec<(&str, i64)> = listed
        .iter()
        .map(|r| (r.rkey.as_str(), r.indexed_at))
        .collect();
    // Newest first, the unchanged one where it was, first seen at 100.
    assert_eq!(order, [("two", 200), ("three", 200), ("one", 100)]);
    // A whole read without one takes it out.
    let third = BTreeMap::from([("a.b.c/one".to_string(), put("one"))]);
    store
        .index(&space, "did:plc:writer", &head("3l3"), true, &third, 300)
        .unwrap();
    let listed = store.indexed(&space, None, None, 10).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (listed[0].rkey.as_str(), listed[0].indexed_at),
        ("one", 100)
    );
}
