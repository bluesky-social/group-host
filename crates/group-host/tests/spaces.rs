//! A group's repos in spaces: written a revision at a time, each write
//! told to the space's authority, and read by a syncer holding the
//! authority's credential, each answer vouched for with a MAC.

mod common;

use atproto::plc::Op;
use atproto::service_auth;
use atproto::space::{
    COMMIT_MAC, Commit, LtHash, RECORD_MAC, element, encapsulate, mac_header, record_mac, star,
    want_mac_header,
};
use atproto::space_auth::{self, Kind, Make, sign_request};
use atproto::{Cid, Curve, Keypair, Value, drisl};
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use common::{Running, serve, start};
use group_host::repo::Write;
use group_host::store::{Account, Change, Head, SpaceHead, Store};
use group_host::time::now_millis;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const RIDERS: &str = "riders.groups.test";
const RULE: &str = "fyi.opensocial.rule";

fn now() -> u64 {
    now_millis() as u64 / 1000
}

fn rule(title: &str) -> Json {
    json!({"$type": RULE, "title": title, "createdAt": "2026-11-03T00:00:00Z"})
}

fn create(collection: &str, rkey: &str, value: Json) -> Write {
    Write::Create {
        collection: collection.into(),
        rkey: rkey.into(),
        value,
    }
}

/// What an authority's PDS was told: each notifyWrite's authorization
/// and body.
#[derive(Default)]
struct Told {
    writes: Mutex<Vec<(String, Json)>>,
    /// Refuses every notification.
    refuse: bool,
}

async fn notify_write(
    State(told): State<Arc<Told>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Json>,
) -> StatusCode {
    let auth = headers["authorization"].to_str().unwrap().to_string();
    told.writes.lock().unwrap().push((auth, body));
    if told.refuse {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    }
}

/// A space's authority elsewhere: an account on the directory, its PDS
/// here, hearing of writes.
struct Authority {
    did: String,
    key: Keypair,
    told: Arc<Told>,
    /// Its rotation key, its PDS, and its last operation, to change its
    /// document with.
    rotation: Keypair,
    pds: String,
    last: Option<Cid>,
}

async fn authority(h: &Running, refuse: bool, pds: bool) -> Authority {
    let told = Arc::new(Told {
        refuse,
        ..Told::default()
    });
    let router = Router::new()
        .route("/xrpc/com.atproto.space.notifyWrite", post(notify_write))
        .with_state(told.clone());
    let url = serve(router).await;
    let key = Keypair::generate(Curve::K256);
    let rotation = Keypair::generate(Curve::K256);
    let mut op = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        "authority.test",
        &url,
    );
    if !pds {
        op.services.clear();
    }
    let genesis = op.sign(None, &rotation);
    let did = genesis.did();
    let res = h
        .http
        .post(format!("{}/{did}", plc_url(h)))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    Authority {
        did,
        key,
        told,
        rotation,
        pds: url,
        last: Some(genesis.cid()),
    }
}

impl Authority {
    /// Signs with a new key from now on, its document saying so.
    async fn rotate(&mut self, h: &Running) {
        self.key = Keypair::generate(Curve::K256);
        let op = Op::atproto(
            &self.key.public().did_key(),
            &[self.rotation.public().did_key()],
            "authority.test",
            &self.pds,
        );
        let signed = op.sign(self.last, &self.rotation);
        let res = h
            .http
            .post(format!("{}/{}", plc_url(h), self.did))
            .json(&signed.to_json())
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success());
        self.last = Some(signed.cid());
    }
}

fn plc_url(h: &Running) -> String {
    h.host.config.plc_url.clone()
}

/// A syncer holding a credential to `space`, bound to its key.
struct Syncer {
    key: Keypair,
    credential: String,
}

fn credential(authority: &Authority, space: &str, key: &Keypair, iss: &str) -> String {
    let make = Make {
        iss,
        sub: space,
        key_id: Some(&key.public().did_key()),
        ..Make::default()
    };
    space_auth::create(Kind::Credential, &make, &authority.key, now()).unwrap()
}

fn syncer(authority: &Authority, space: &str) -> Syncer {
    let key = Keypair::generate(Curve::P256);
    let credential = credential(authority, space, &key, &authority.did);
    Syncer { key, credential }
}

/// What a read answers: its status, headers and body.
struct Read {
    status: u16,
    headers: reqwest::header::HeaderMap,
    body: Vec<u8>,
}

impl Read {
    fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap()
    }

    fn error(&self) -> (u16, String) {
        (self.status, self.json()["error"].as_str().unwrap().into())
    }

    fn mac(&self) -> Option<&str> {
        self.headers
            .get("atproto-space-mac")
            .map(|v| v.to_str().unwrap())
    }
}

/// `GET nsid?query` with `headers`.
async fn read(h: &Running, nsid: &str, query: &str, headers: &[(&str, String)]) -> Read {
    let mut req = h.http.get(format!("{}/xrpc/{nsid}?{query}", h.url));
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let res = req.send().await.unwrap();
    Read {
        status: res.status().as_u16(),
        headers: res.headers().clone(),
        body: res.bytes().await.unwrap().to_vec(),
    }
}

/// The headers a syncer reads `repo` with: its credential, signed, and a
/// key share for a MAC of `kind`; and the secret agreed.
fn signed(
    s: &Syncer,
    repo: &str,
    kem: &str,
    kind: &str,
) -> (Vec<(&'static str, String)>, [u8; 32]) {
    let authorization = format!("Atproto-Space-Credential {}", s.credential);
    let mut headers: Vec<(&str, String)> = sign_request(&s.key, &authorization, repo)
        .unwrap()
        .into_iter()
        .collect();
    let (share, secret) = encapsulate(kem).unwrap();
    headers.push(("atproto-space-want-mac", want_mac_header(kind, &share)));
    (headers, secret)
}

/// A group, a space of an authority elsewhere, and a syncer of it.
struct Fixture {
    h: Running,
    group: String,
    kem: String,
    authority: Authority,
    space: String,
    syncer: Syncer,
}

async fn fixture() -> Fixture {
    let h = start(&[], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    let kem = h
        .host
        .store
        .account(&group)
        .unwrap()
        .unwrap()
        .kem
        .public()
        .did_key();
    let authority = authority(&h, false, true).await;
    let space = format!("at://{}/space/fyi.opensocial.content/self", authority.did);
    let syncer = syncer(&authority, &space);
    Fixture {
        h,
        group,
        kem,
        authority,
        space,
        syncer,
    }
}

impl Fixture {
    fn query(&self, rest: &str) -> String {
        format!("space={}&repo={}{rest}", self.space, self.group)
    }

    async fn get(&self, nsid: &str, rest: &str, kind: &str) -> (Read, [u8; 32]) {
        let (headers, secret) = signed(&self.syncer, &self.group, &self.kem, kind);
        (
            read(&self.h, nsid, &self.query(rest), &headers).await,
            secret,
        )
    }
}

#[tokio::test]
async fn writes_a_repo_in_a_space_and_tells_its_authority() {
    let f = fixture().await;
    let first =
        f.h.host
            .space_write(
                &f.group,
                &f.space,
                &[
                    create(RULE, "one", rule("Be kind")),
                    create(RULE, "two", rule("No spam")),
                ],
            )
            .await
            .unwrap();
    let second =
        f.h.host
            .space_write(
                &f.group,
                &f.space,
                &[
                    Write::Update {
                        collection: RULE.into(),
                        rkey: "one".into(),
                        value: rule("Be kinder"),
                    },
                    Write::Delete {
                        collection: RULE.into(),
                        rkey: "two".into(),
                    },
                ],
            )
            .await
            .unwrap();
    assert!(second.rev > first.rev);

    // The commit is the set hash of the records there now.
    let bytes = drisl::encode(&Value::from_json(&rule("Be kinder")).unwrap());
    let mut hash = LtHash::default();
    hash.add(&element(RULE, "one", &Cid::of_cbor(&bytes)));
    assert_eq!(
        second,
        Commit {
            repo: f.group.clone(),
            space: f.space.clone(),
            rev: second.rev.clone(),
            data: hash.digest(),
        }
    );

    // The authority heard of each, from the group, as the group.
    let told = f.authority.told.writes.lock().unwrap().clone();
    assert_eq!(told.len(), 2);
    let (auth, body) = &told[1];
    assert_eq!(
        body,
        &json!({
            "space": f.space,
            "repo": f.group,
            "repoRev": second.rev,
            "data": Value::Bytes(second.data.to_vec()).to_json(),
        })
    );
    let token = auth.strip_prefix("Bearer ").unwrap();
    let account = f.h.host.store.account(&f.group).unwrap().unwrap();
    let claims = service_auth::verify(token, &account.signing.public(), now()).unwrap();
    let aud = format!("{}#atproto_pds", f.authority.did);
    assert_eq!(
        service_auth::check(&claims, &aud, "com.atproto.space.notifyWrite").unwrap(),
        f.group
    );

    // Writes the repo's own checks refuse are refused here too.
    let refused =
        f.h.host
            .space_write(&f.group, &f.space, &[create(RULE, "one", rule("Again"))])
            .await
            .unwrap_err();
    assert_eq!(
        refused.message,
        format!("there is a record already at {RULE}/one")
    );
    let refused =
        f.h.host
            .space_write(&f.group, "at://riders/space", &[])
            .await
            .unwrap_err();
    assert_eq!(refused.message, "not a space: at://riders/space");
    let refused =
        f.h.host
            .space_write("did:plc:nobody", &f.space, &[])
            .await
            .unwrap_err();
    assert_eq!(
        (refused.error, refused.message.as_str()),
        ("RepoNotFound", "no repo did:plc:nobody here")
    );
}

#[tokio::test]
async fn a_write_stands_when_its_authority_does_not_hear_of_it() {
    let h = start(&[], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    let refusing = authority(&h, true, true).await;
    let pdsless = authority(&h, false, false).await;
    for space in [
        format!("at://{}/space/fyi.opensocial.content/self", refusing.did),
        format!("at://{}/space/fyi.opensocial.content/self", pdsless.did),
        "at://did:plc:unpublished/space/fyi.opensocial.content/self".to_string(),
    ] {
        let written = h
            .host
            .space_write(&group, &space, &[create(RULE, "one", rule("Be kind"))])
            .await;
        assert!(written.is_ok(), "{space}");
    }
    assert_eq!(refusing.told.writes.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_write_waits_on_its_authority_no_longer_than_the_timeout() {
    let h = start(&[], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    // A PDS that takes the request and never answers.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    let rotation = Keypair::generate(Curve::K256);
    let key = Keypair::generate(Curve::K256);
    let op = Op::atproto(
        &key.public().did_key(),
        &[rotation.public().did_key()],
        "silent.test",
        &url,
    );
    let genesis = op.sign(None, &rotation);
    let did = genesis.did();
    h.http
        .post(format!("{}/{did}", plc_url(&h)))
        .json(&genesis.to_json())
        .send()
        .await
        .unwrap();
    let space = format!("at://{did}/space/fyi.opensocial.content/self");
    let started = std::time::Instant::now();
    h.host
        .space_write(&group, &space, &[create(RULE, "one", rule("Be kind"))])
        .await
        .unwrap();
    let waited = started.elapsed();
    assert!(waited >= group_host::OUTGOING_TIMEOUT, "{waited:?}");
    assert!(waited < group_host::OUTGOING_TIMEOUT * 2, "{waited:?}");
}

#[tokio::test]
async fn serves_a_repo_to_a_syncer_vouched_for() {
    let f = fixture().await;
    let writes: Vec<Write> = ["a", "b", "c"]
        .iter()
        .map(|k| create(RULE, k, rule("Be kind")))
        .chain([create(
            "fyi.opensocial.ruleset",
            "self",
            json!({"$type": "fyi.opensocial.ruleset", "name": "Rules"}),
        )])
        .collect();
    let commit =
        f.h.host
            .space_write(&f.group, &f.space, &writes)
            .await
            .unwrap();

    // The whole repo, as an archive of its commit and records, the commit
    // vouched for under the secret the syncer agreed.
    let (repo, secret) = f.get("com.atproto.space.getRepo", "", COMMIT_MAC).await;
    assert_eq!(repo.status, 200);
    let records: BTreeMap<String, Vec<u8>> = writes
        .iter()
        .map(|w| {
            let Write::Create {
                collection,
                rkey,
                value,
            } = w
            else {
                unreachable!()
            };
            (
                format!("{collection}/{rkey}"),
                drisl::encode(&Value::from_json(value).unwrap()),
            )
        })
        .collect();
    assert_eq!(repo.body, star(&commit, &records));
    assert_eq!(
        repo.mac(),
        Some(mac_header(COMMIT_MAC, &commit.mac(&secret)).as_str())
    );
    assert_eq!(repo.headers["cache-control"], "private");
    assert_eq!(
        repo.headers["vary"],
        "authorization, atproto-space-want-mac"
    );

    let (latest, secret) = f
        .get("com.atproto.space.getLatestCommit", "", COMMIT_MAC)
        .await;
    assert_eq!(
        latest.json(),
        json!({"commit": {
            "ver": 1,
            "repo": f.group,
            "space": f.space,
            "rev": commit.rev,
            "data": Value::Bytes(commit.data.to_vec()).to_json(),
        }})
    );
    assert_eq!(
        latest.mac(),
        Some(mac_header(COMMIT_MAC, &commit.mac(&secret)).as_str())
    );

    // One record, vouched for by its URI and CID.
    let (one, secret) = f
        .get(
            "com.atproto.space.getRecord",
            &format!("&collection={RULE}&rkey=b"),
            RECORD_MAC,
        )
        .await;
    let uri = format!("{}/{}/{RULE}/b", f.space, f.group);
    let cid = Cid::of_cbor(&records[&format!("{RULE}/b")]);
    assert_eq!(
        one.json(),
        json!({"uri": uri, "cid": cid.to_string(), "value": rule("Be kind")})
    );
    assert_eq!(
        one.mac(),
        Some(mac_header(RECORD_MAC, &record_mac(&secret, &uri, &cid)).as_str())
    );
    let (none, _) = f
        .get(
            "com.atproto.space.getRecord",
            &format!("&collection={RULE}&rkey=z"),
            RECORD_MAC,
        )
        .await;
    assert_eq!(none.error(), (400, "RecordNotFound".into()));

    // A group with no repo in the space has none to give.
    let other = f.h.host.create_account("hikers.groups.test").await.unwrap();
    let kem =
        f.h.host
            .store
            .account(&other)
            .unwrap()
            .unwrap()
            .kem
            .public()
            .did_key();
    for nsid in ["getRepo", "getLatestCommit"] {
        let (headers, _) = signed(&f.syncer, &other, &kem, COMMIT_MAC);
        let query = format!("space={}&repo={other}", f.space);
        let got = read(&f.h, &format!("com.atproto.space.{nsid}"), &query, &headers).await;
        assert_eq!(got.error(), (400, "RepoNotFound".into()), "{nsid}");
    }
    let (headers, _) = signed(&f.syncer, &other, &kem, COMMIT_MAC);
    let query = format!("space={}&repo={other}", f.space);
    let ops = read(&f.h, "com.atproto.space.listRepoOps", &query, &headers).await;
    assert_eq!(
        ops.json(),
        json!({"ops": [], "cursor": null, "commit": null})
    );
    assert_eq!(ops.mac(), None);
}

async fn list(f: &Fixture, rest: String) -> Json {
    let (got, _) = f
        .get("com.atproto.space.listRecords", &rest, COMMIT_MAC)
        .await;
    got.json()
}

#[tokio::test]
async fn lists_a_repos_records_a_page_at_a_time() {
    let f = fixture().await;
    // `fyi.opensocial.rule.x/…` sorts before `fyi.opensocial.rule/…` by
    // path, as `.` comes before `/`.
    let writes = [
        create(RULE, "a", rule("A")),
        create(RULE, "b", rule("B")),
        create(
            "fyi.opensocial.rule.x",
            "a",
            json!({"$type": "fyi.opensocial.rule.x"}),
        ),
    ];
    f.h.host
        .space_write(&f.group, &f.space, &writes)
        .await
        .unwrap();
    let paths = |page: &Json| -> Vec<String> {
        page["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                format!(
                    "{}/{}",
                    r["collection"].as_str().unwrap(),
                    r["rkey"].as_str().unwrap()
                )
            })
            .collect()
    };
    let all = list(&f, String::new()).await;
    assert_eq!(
        paths(&all),
        [
            format!("{RULE}/b"),
            format!("{RULE}/a"),
            "fyi.opensocial.rule.x/a".into()
        ]
    );
    assert_eq!(all["records"][0]["value"], rule("B"));
    assert_eq!(all["cursor"], Json::Null);

    let first = list(&f, "&limit=2&reverse=true".into()).await;
    assert_eq!(
        paths(&first),
        ["fyi.opensocial.rule.x/a".to_string(), format!("{RULE}/a")]
    );
    let cursor = first["cursor"].as_str().unwrap().to_string();
    assert_eq!(cursor, format!("{}/{}/{RULE}/a", f.space, f.group));
    let rest = list(&f, format!("&limit=2&reverse=true&cursor={cursor}")).await;
    assert_eq!(paths(&rest), [format!("{RULE}/b")]);

    let rules = list(&f, format!("&collection={RULE}&excludeValues=true")).await;
    assert_eq!(paths(&rules), [format!("{RULE}/b"), format!("{RULE}/a")]);
    assert_eq!(rules["records"][0].get("value"), None);

    for (rest, why) in [
        (
            "&cursor=at://elsewhere/x/y",
            "the cursor is not a record of this repo",
        ),
        ("&limit=1001", "limit is 1 to 1000"),
    ] {
        let (got, _) = f
            .get("com.atproto.space.listRecords", rest, COMMIT_MAC)
            .await;
        assert_eq!(got.status, 400);
        assert_eq!(got.json()["message"], why);
    }
}

#[tokio::test]
async fn lists_a_repos_changes_and_vouches_for_where_they_lead() {
    let f = fixture().await;
    let first =
        f.h.host
            .space_write(
                &f.group,
                &f.space,
                &[create(RULE, "a", rule("A")), create(RULE, "b", rule("B"))],
            )
            .await
            .unwrap();
    let second =
        f.h.host
            .space_write(
                &f.group,
                &f.space,
                &[
                    Write::Update {
                        collection: RULE.into(),
                        rkey: "a".into(),
                        value: rule("A again"),
                    },
                    Write::Delete {
                        collection: RULE.into(),
                        rkey: "b".into(),
                    },
                ],
            )
            .await
            .unwrap();
    let cid = |title: &str| {
        Cid::of_cbor(&drisl::encode(&Value::from_json(&rule(title)).unwrap())).to_string()
    };

    let (all, secret) = f.get("com.atproto.space.listRepoOps", "", COMMIT_MAC).await;
    let all_json = all.json();
    // An op superseded since has no value: the record is not what it made.
    assert_eq!(
        all_json["ops"],
        json!([
            {"rev": first.rev, "collection": RULE, "rkey": "a", "cid": cid("A"), "prev": null},
            {"rev": first.rev, "collection": RULE, "rkey": "b", "cid": cid("B"), "prev": null},
            {"rev": second.rev, "collection": RULE, "rkey": "a", "cid": cid("A again"), "prev": cid("A"), "value": rule("A again")},
            {"rev": second.rev, "collection": RULE, "rkey": "b", "cid": null, "prev": cid("B")},
        ])
    );
    assert_eq!(all_json["cursor"], Json::Null);
    assert_eq!(all_json["commit"]["rev"], second.rev);
    assert_eq!(
        all.mac(),
        Some(mac_header(COMMIT_MAC, &second.mac(&secret)).as_str())
    );

    // A page at a time: a full page has a cursor, and no commit.
    let (page, _) = f
        .get("com.atproto.space.listRepoOps", "&limit=3", COMMIT_MAC)
        .await;
    let page = page.json();
    assert_eq!(page["ops"].as_array().unwrap().len(), 3);
    assert_eq!(page["commit"], Json::Null);
    let cursor = page["cursor"].as_str().unwrap();
    assert_eq!(cursor, format!("{}/0", second.rev));
    let (last, _) = f
        .get(
            "com.atproto.space.listRepoOps",
            &format!("&limit=3&cursor={cursor}&excludeValues=true"),
            COMMIT_MAC,
        )
        .await;
    let last = last.json();
    assert_eq!(last["ops"], json!([all_json["ops"][3]]));
    assert_eq!(last["commit"], all_json["commit"]);

    // Since a revision, the ops after it.
    let (since, _) = f
        .get(
            "com.atproto.space.listRepoOps",
            &format!("&since={}&excludeValues=true", first.rev),
            COMMIT_MAC,
        )
        .await;
    let since = since.json();
    assert_eq!(since["ops"].as_array().unwrap().len(), 2);
    assert_eq!(since["ops"][0].get("value"), None);

    for cursor in ["nope", "rev/x"] {
        let (bad, _) = f
            .get(
                "com.atproto.space.listRepoOps",
                &format!("&cursor={cursor}"),
                COMMIT_MAC,
            )
            .await;
        assert_eq!(bad.error(), (400, "MalformedCursor".into()));
    }
}

#[tokio::test]
async fn serves_and_lists_a_spaces_blobs() {
    let f = fixture().await;
    let picture =
        f.h.host
            .upload_blob(&f.group, b"picture", "image/png")
            .unwrap();
    let elsewhere =
        f.h.host
            .upload_blob(&f.group, b"elsewhere", "image/png")
            .unwrap();
    let link = |r: &Json| r["ref"]["$link"].as_str().unwrap().to_string();
    let profile = |avatar: &Json| json!({"$type": "fyi.opensocial.profile", "displayName": "Riders", "avatar": avatar});
    let first =
        f.h.host
            .space_write(
                &f.group,
                &f.space,
                &[create("fyi.opensocial.profile", "self", profile(&picture))],
            )
            .await
            .unwrap();
    // A blob a record elsewhere references is not this space's.
    f.h.host
        .write(
            &f.group,
            &[create(
                "fyi.opensocial.profile",
                "self",
                profile(&elsewhere),
            )],
        )
        .unwrap();

    let (got, _) = f
        .get(
            "com.atproto.space.getBlob",
            &format!("&cid={}", link(&picture)),
            COMMIT_MAC,
        )
        .await;
    assert_eq!(got.status, 200);
    assert_eq!(got.body, b"picture");
    assert_eq!(got.headers["content-type"], "image/png");
    assert_eq!(got.headers["x-content-type-options"], "nosniff");
    for cid in [
        link(&elsewhere),
        "bafkreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm".into(),
    ] {
        let (none, _) = f
            .get(
                "com.atproto.space.getBlob",
                &format!("&cid={cid}"),
                COMMIT_MAC,
            )
            .await;
        assert_eq!(none.error(), (400, "BlobNotFound".into()));
    }
    let (bad, _) = f
        .get("com.atproto.space.getBlob", "&cid=nope", COMMIT_MAC)
        .await;
    assert_eq!(bad.status, 400);

    let (listed, _) = f.get("com.atproto.space.listBlobs", "", COMMIT_MAC).await;
    assert_eq!(
        listed.json(),
        json!({"cids": [link(&picture)], "cursor": null})
    );
    let (page, _) = f
        .get("com.atproto.space.listBlobs", "&limit=1", COMMIT_MAC)
        .await;
    assert_eq!(page.json()["cursor"], link(&picture));
    let (since, _) = f
        .get(
            "com.atproto.space.listBlobs",
            &format!("&since={}", first.rev),
            COMMIT_MAC,
        )
        .await;
    assert_eq!(since.json()["cids"], json!([]));

    // Once no record references it, it is gone.
    f.h.host
        .space_write(
            &f.group,
            &f.space,
            &[Write::Delete {
                collection: "fyi.opensocial.profile".into(),
                rkey: "self".into(),
            }],
        )
        .await
        .unwrap();
    let cid = Cid::parse(&link(&picture)).unwrap();
    assert_eq!(f.h.host.store.blob(&f.group, &cid).unwrap(), None);
}

/// A read of the fixture's repo with `headers` as given.
async fn get_with(f: &Fixture, headers: &[(&str, String)]) -> (u16, String) {
    read(
        &f.h,
        "com.atproto.space.getLatestCommit",
        &f.query(""),
        headers,
    )
    .await
    .error()
}

#[tokio::test]
async fn reads_only_with_a_credential_the_authority_issued() {
    let f = fixture().await;
    f.h.host
        .space_write(&f.group, &f.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    let (good, _) = signed(&f.syncer, &f.group, &f.kem, COMMIT_MAC);
    let with = |name: &str, value: String| -> Vec<(&'static str, String)> {
        good.iter()
            .map(|(k, v)| (*k, if *k == name { value.clone() } else { v.clone() }))
            .collect()
    };
    let without = |name: &str| -> Vec<(&'static str, String)> {
        good.iter().filter(|(k, _)| *k != name).cloned().collect()
    };
    let bearing = |credential: &str, key: &Keypair| {
        let s = Syncer {
            key: key.clone(),
            credential: credential.into(),
        };
        signed(&s, &f.group, &f.kem, COMMIT_MAC).0
    };
    let other_key = Keypair::generate(Curve::P256);
    let elsewhere = format!(
        "at://{}/space/fyi.opensocial.content/other",
        f.authority.did
    );
    let stranger = Authority {
        did: f.authority.did.clone(),
        key: Keypair::generate(Curve::K256),
        told: Default::default(),
        rotation: Keypair::generate(Curve::K256),
        pds: String::new(),
        last: None,
    };
    let expired = space_auth::create(
        Kind::Credential,
        &Make {
            iss: &f.authority.did,
            sub: &f.space,
            key_id: Some(&f.syncer.key.public().did_key()),
            ..Make::default()
        },
        &f.authority.key,
        now() - 7200,
    )
    .unwrap();
    for (headers, refused) in [
        (without("authorization"), (401, "MissingJwt")),
        (
            with("authorization", format!("Bearer {}", f.syncer.credential)),
            (401, "MissingJwt"),
        ),
        (
            with("authorization", "Atproto-Space-Credential x.y".into()),
            (401, "BadJwt"),
        ),
        // Issued for a space that is not the authority's.
        (
            bearing(
                &credential(
                    &f.authority,
                    "at://nowhere",
                    &f.syncer.key,
                    &f.authority.did,
                ),
                &f.syncer.key,
            ),
            (401, "BadJwtSub"),
        ),
        // Issued by someone not the space's authority.
        (
            bearing(
                &credential(&f.authority, &f.space, &f.syncer.key, "did:plc:someone"),
                &f.syncer.key,
            ),
            (401, "BadJwtIss"),
        ),
        // Of an authority not on the directory.
        (
            bearing(
                &credential(
                    &f.authority,
                    "at://did:plc:unknown/space/fyi.opensocial.content/self",
                    &f.syncer.key,
                    "did:plc:unknown",
                ),
                &f.syncer.key,
            ),
            (401, "BadJwtIss"),
        ),
        // Signed by another key than the authority's.
        (
            bearing(
                &credential(&stranger, &f.space, &f.syncer.key, &f.authority.did),
                &f.syncer.key,
            ),
            (401, "BadJwtSignature"),
        ),
        (bearing(&expired, &f.syncer.key), (401, "JwtExpired")),
        // For another space of the same authority.
        (
            bearing(
                &credential(&f.authority, &elsewhere, &f.syncer.key, &f.authority.did),
                &f.syncer.key,
            ),
            (401, "BadJwtSub"),
        ),
        // For the space, but sent for another repo.
        (
            with("atproto-space-audience", "did:plc:other".into()),
            (400, "BadSpaceAudience"),
        ),
        // The request signed with a key the credential is not bound to.
        (
            bearing(&f.syncer.credential, &other_key),
            (401, "BadSignature"),
        ),
        (without("signature"), (401, "BadSignature")),
        (without("atproto-space-want-mac"), (400, "BadSpaceMac")),
        (
            with(
                "atproto-space-want-mac",
                want_mac_header(COMMIT_MAC, &other_key.public().did_key())
                    .replace("ver=1", "ver=2"),
            ),
            (400, "BadSpaceMac"),
        ),
    ] {
        let headers: Vec<(&str, String)> = headers;
        assert_eq!(
            get_with(&f, &headers).await,
            (refused.0, refused.1.to_string()),
            "{headers:?}"
        );
    }
    let fine = read(
        &f.h,
        "com.atproto.space.getLatestCommit",
        &f.query(""),
        &good,
    )
    .await;
    assert_eq!(fine.status, 200);

    // A repo not here, and a space that is not one.
    let (headers, _) = signed(&f.syncer, "did:plc:nobody", &f.kem, COMMIT_MAC);
    let query = format!("space={}&repo=did:plc:nobody", f.space);
    let got = read(&f.h, "com.atproto.space.getRepo", &query, &headers).await;
    assert_eq!(got.error(), (400, "RepoNotFound".into()));
    let got = read(
        &f.h,
        "com.atproto.space.getRepo",
        "space=nope&repo=x",
        &good,
    )
    .await;
    assert_eq!(got.error(), (400, "InvalidRequest".into()));
    let got = read(
        &f.h,
        "com.atproto.space.getRepo",
        &format!("space={}", f.space),
        &good,
    )
    .await;
    assert_eq!(got.json()["message"], "repo is required");
}

/// `notifyCredentialRevoked` with `body`, as `iss` for `aud`, signed with
/// `key`, for `lxm`.
async fn revoke(
    f: &Fixture,
    body: &Json,
    iss: &str,
    aud: &str,
    key: &Keypair,
    lxm: &str,
) -> (u16, Json) {
    let token = service_auth::sign(
        key,
        &service_auth::Claims {
            iss: iss.into(),
            aud: aud.into(),
            exp: now() + 60,
            iat: Some(now()),
            lxm: Some(lxm.into()),
            jti: None,
        },
    );
    let res =
        f.h.http
            .post(format!(
                "{}/xrpc/com.atproto.space.notifyCredentialRevoked",
                f.h.url
            ))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .unwrap();
    (res.status().as_u16(), res.json().await.unwrap())
}

#[tokio::test]
async fn reads_no_more_with_a_credential_its_authority_revoked() {
    let f = fixture().await;
    f.h.host
        .space_write(&f.group, &f.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    let (headers, _) = signed(&f.syncer, &f.group, &f.kem, COMMIT_MAC);
    assert_eq!(
        read(
            &f.h,
            "com.atproto.space.getLatestCommit",
            &f.query(""),
            &headers
        )
        .await
        .status,
        200
    );
    let (_, token) = space_auth::parse(Kind::Credential, &f.syncer.credential).unwrap();
    let body = json!({"space": f.space, "credentials": [token.jti]});
    let lxm = "com.atproto.space.notifyCredentialRevoked";
    let a = &f.authority;

    let error = |r: (u16, Json)| (r.0, r.1["error"].as_str().unwrap_or_default().to_string());
    let other = Keypair::generate(Curve::K256);
    for (said, refused) in [
        (
            revoke(
                &f,
                &body,
                &a.did,
                &f.group,
                &a.key,
                "com.atproto.space.notifyWrite",
            )
            .await,
            (401, "BadJwtLexiconMethod"),
        ),
        (
            revoke(&f, &body, &a.did, &f.group, &other, lxm).await,
            (401, "BadJwt"),
        ),
        (
            revoke(&f, &body, "did:plc:unknown", &f.group, &a.key, lxm).await,
            (401, "BadJwtIss"),
        ),
        (
            revoke(
                &f,
                &body,
                &f.group,
                &f.group,
                &f.h.host.store.account(&f.group).unwrap().unwrap().signing,
                lxm,
            )
            .await,
            (403, "Forbidden"),
        ),
        (
            revoke(&f, &body, &a.did, "did:plc:elsewhere", &a.key, lxm).await,
            (403, "Forbidden"),
        ),
        (
            revoke(
                &f,
                &json!({"space": "nope", "credentials": ["x"]}),
                &a.did,
                &f.group,
                &a.key,
                lxm,
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            revoke(
                &f,
                &json!({"space": f.space, "credentials": []}),
                &a.did,
                &f.group,
                &a.key,
                lxm,
            )
            .await,
            (400, "InvalidRequest"),
        ),
        (
            revoke(
                &f,
                &json!({"space": f.space, "credentials": [""]}),
                &a.did,
                &f.group,
                &a.key,
                lxm,
            )
            .await,
            (400, "InvalidRequest"),
        ),
    ] {
        assert_eq!(error(said), (refused.0, refused.1.to_string()));
    }
    let res =
        f.h.http
            .post(format!(
                "{}/xrpc/com.atproto.space.notifyCredentialRevoked",
                f.h.url
            ))
            .json(&body)
            .send()
            .await
            .unwrap();
    assert_eq!(res.status(), 401);

    // Still readable until the authority itself says otherwise.
    let (headers, _) = signed(&f.syncer, &f.group, &f.kem, COMMIT_MAC);
    assert_eq!(
        read(
            &f.h,
            "com.atproto.space.getLatestCommit",
            &f.query(""),
            &headers
        )
        .await
        .status,
        200
    );
    assert_eq!(
        revoke(&f, &body, &a.did, &f.group, &a.key, lxm).await,
        (200, json!({}))
    );
    let (headers, _) = signed(&f.syncer, &f.group, &f.kem, COMMIT_MAC);
    let got = read(
        &f.h,
        "com.atproto.space.getLatestCommit",
        &f.query(""),
        &headers,
    )
    .await;
    assert_eq!(got.error(), (401, "CredentialRevoked".into()));
    // Another credential to the same space reads on.
    let fresh = syncer(&f.authority, &f.space);
    let (headers, _) = signed(&fresh, &f.group, &f.kem, COMMIT_MAC);
    assert_eq!(
        read(
            &f.h,
            "com.atproto.space.getLatestCommit",
            &f.query(""),
            &headers
        )
        .await
        .status,
        200
    );
}

#[test]
fn a_space_write_builds_on_the_revision_it_read() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let account = Account {
        did: "did:plc:a".into(),
        handle: "a.groups.test".into(),
        signing: Keypair::generate(Curve::K256),
        kem: Keypair::generate(Curve::P256),
    };
    let tree = atproto::mst::Tree::build(&Default::default());
    let commit =
        atproto::Commit::sign(&account.did, tree.root(), "3l5mz5mkbn22b", &account.signing);
    let first = Head {
        cid: commit.cid(),
        rev: commit.rev.clone(),
        block: commit.to_bytes(),
    };
    assert!(store.create_account(&account, &first, 0, &[]).unwrap());
    let head = SpaceHead {
        rev: "3l5mz5mkbn22b".into(),
        state: LtHash::default().state(),
    };
    let changes: [Change; 0] = [];
    assert!(
        store
            .space_commit("did:plc:a", "s", None, &head, &changes)
            .unwrap()
            .is_some()
    );
    // A second first write, or one from a revision no longer the latest,
    // changes nothing.
    assert!(
        store
            .space_commit("did:plc:a", "s", None, &head, &changes)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .space_commit("did:plc:a", "s", Some("2222222222222"), &head, &changes)
            .unwrap()
            .is_none()
    );
    assert_eq!(store.space_head("did:plc:a", "s").unwrap(), Some(head));
}

/// What `getLatestCommit` answers the fixture's syncer.
async fn latest(f: &Fixture) -> u16 {
    let (headers, _) = signed(&f.syncer, &f.group, &f.kem, COMMIT_MAC);
    read(
        &f.h,
        "com.atproto.space.getLatestCommit",
        &f.query(""),
        &headers,
    )
    .await
    .status
}

#[tokio::test]
async fn reads_with_a_credential_signed_by_an_authoritys_new_key() {
    let mut f = fixture().await;
    f.h.host
        .space_write(&f.group, &f.space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    // The host keeps the authority's key it read.
    assert_eq!(latest(&f).await, 200);
    // The authority moves to a new key: a credential it signs with that
    // one is checked against its document read again.
    f.authority.rotate(&f.h).await;
    f.syncer = syncer(&f.authority, &f.space);
    assert_eq!(latest(&f).await, 200);
}

/// A did:web authority's server: its document, and its PDS hearing of
/// writes.
async fn web_authority(told: Arc<Told>) -> (String, Keypair) {
    let doc = Arc::new(Mutex::new(Json::Null));
    let served = doc.clone();
    let router = Router::new()
        .route("/xrpc/com.atproto.space.notifyWrite", post(notify_write))
        .with_state(told)
        .route(
            "/.well-known/did.json",
            axum::routing::get(move || {
                let doc = served.lock().unwrap().clone();
                async move { axum::Json(doc) }
            }),
        );
    let url = serve(router).await;
    let did = format!(
        "did:web:{}",
        url.strip_prefix("http://").unwrap().replace(':', "%3A")
    );
    let key = Keypair::generate(Curve::K256);
    *doc.lock().unwrap() = atproto::did::service_document(
        &did,
        &key.public(),
        &[("#atproto_pds", "AtprotoPersonalDataServer", url.as_str())],
    );
    (did, key)
}

#[tokio::test]
async fn a_space_of_a_did_web_authority() {
    let h = start(&[("DID_WEB_SCHEME", "http")], false).await;
    let group = h.host.create_account(RIDERS).await.unwrap();
    let kem = h
        .host
        .store
        .account(&group)
        .unwrap()
        .unwrap()
        .kem
        .public()
        .did_key();
    let told = Arc::new(Told::default());
    let (did, key) = web_authority(told.clone()).await;
    let authority = Authority {
        did,
        key,
        told,
        rotation: Keypair::generate(Curve::K256),
        pds: String::new(),
        last: None,
    };
    let space = format!("at://{}/space/fyi.opensocial.content/self", authority.did);
    let commit = h
        .host
        .space_write(&group, &space, &[create(RULE, "a", rule("A"))])
        .await
        .unwrap();
    // Told, at the PDS its document names.
    assert_eq!(
        authority.told.writes.lock().unwrap()[0].1["repoRev"],
        commit.rev
    );
    // And its credential reads.
    let syncer = syncer(&authority, &space);
    let (headers, secret) = signed(&syncer, &group, &kem, COMMIT_MAC);
    // A did:web's `%3A` is sent as it is, escaped.
    let query = format!("space={}&repo={group}", space.replace('%', "%25"));
    let got = read(&h, "com.atproto.space.getLatestCommit", &query, &headers).await;
    assert_eq!(got.status, 200, "{}", String::from_utf8_lossy(&got.body));
    assert_eq!(
        got.mac(),
        Some(mac_header(COMMIT_MAC, &commit.mac(&secret)).as_str())
    );
}
