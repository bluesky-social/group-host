//! The space alpha's primitives, against vectors made by running
//! `@atproto/space` (tests/reference/space-vectors.json, from pds.js, and
//! tests/reference/space-meta.json, from space-meta.mjs beside it).

use atproto::cid::Cid;
use atproto::drisl;
use atproto::keys::{Curve, Keypair};
use atproto::space::{
    COMMIT_MAC, Commit, LtHash, RECORD_MAC, RECORD_MAX, STAR_MAGIC, STATE_BYTES, SpaceRef,
    SpaceWriter, Star, decapsulate, element, encapsulate, mac_header, parse_want_mac,
    read_space_meta, read_star_parts, record_mac, star, want_mac_header, write_star,
};
use serde_json::Value;
use std::collections::BTreeMap;

fn reference(name: &str) -> Value {
    let path = format!("{}/tests/reference/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn vectors() -> Value {
    reference("space-vectors.json")
}

fn hex(v: &Value) -> Vec<u8> {
    let h = v.as_str().unwrap();
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn set_hashes_as_the_vectors_say() {
    let v = vectors();
    let empty = LtHash::default();
    assert_eq!(empty.state(), hex(&v["lthash"]["empty"]["stateHex"]));
    assert_eq!(
        empty.digest().to_vec(),
        hex(&v["lthash"]["empty"]["digestHex"])
    );
    assert!(empty.is_empty());
    for one in v["lthash"]["single"].as_array().unwrap() {
        let mut h = LtHash::default();
        h.add(s(&one["element"]));
        assert_eq!(h.state(), hex(&one["stateHex"]));
        assert_eq!(h.digest().to_vec(), hex(&one["digestHex"]));
        assert_eq!(LtHash::from_state(&h.state()).unwrap(), h);
        h.remove(s(&one["element"]));
        assert!(h.is_empty());
    }
    for set in v["lthash"]["sets"].as_array().unwrap() {
        let mut h = LtHash::default();
        for e in set["elements"].as_array().unwrap() {
            h.add(s(e));
        }
        assert_eq!(h.digest().to_vec(), hex(&set["digestHex"]));
    }
    assert_eq!(
        LtHash::from_state(&[0; 3]).unwrap_err().to_string(),
        format!("an LtHash state is {STATE_BYTES} bytes")
    );
}

#[test]
fn commits_records_and_archives_as_the_vectors_say() {
    let v = vectors();
    let c = &v["commit"];
    let mut h = LtHash::default();
    let mut records = BTreeMap::new();
    for r in c["records"].as_array().unwrap() {
        let cid = Cid::parse(s(&r["cid"])).unwrap();
        h.add(&element(s(&r["collection"]), s(&r["rkey"]), &cid));
        records.insert(
            format!("{}/{}", s(&r["collection"]), s(&r["rkey"])),
            hex(&r["valueHex"]),
        );
    }
    assert_eq!(h.digest().to_vec(), hex(&c["dataHex"]));
    let commit = Commit {
        repo: s(&c["repo"]).into(),
        space: s(&c["space"]).into(),
        rev: s(&c["rev"]).into(),
        data: h.digest(),
    };
    let mac = commit.mac(&hex(&c["secretHex"]));
    assert_eq!(mac.to_vec(), hex(&c["macHex"]));
    assert_eq!(mac_header(COMMIT_MAC, &mac), s(&c["macHeader"]));
    assert_eq!(
        atproto::space::parse_mac_header(s(&c["macHeader"]), COMMIT_MAC).unwrap(),
        mac.to_vec()
    );
    for bad in [
        "type=\"atproto-space-record\", ver=1, mac=:AAAA:",
        "type=\"atproto-space-repo\", ver=2, mac=:AAAA:",
        "type=\"atproto-space-repo\", ver=1",
        "type=\"atproto-space-repo\", ver=1, mac=:not base64!:",
    ] {
        assert_eq!(
            atproto::space::parse_mac_header(bad, COMMIT_MAC)
                .unwrap_err()
                .to_string(),
            "not a MAC header of atproto-space-repo",
            "{bad}"
        );
    }
    assert_eq!(star(&commit, &records), hex(&c["starHex"]));

    let r = &v["record"];
    let mac = record_mac(
        &hex(&r["secretHex"]),
        s(&r["uri"]),
        &Cid::parse(s(&r["cid"])).unwrap(),
    );
    assert_eq!(mac.to_vec(), hex(&r["macHex"]));
    assert_eq!(mac_header(RECORD_MAC, &mac), s(&r["macHeader"]));
}

#[test]
fn agrees_secrets_as_the_vectors_say() {
    let v = vectors();
    let k = &v["kem"];
    let author = Keypair::from_bytes(Curve::P256, &hex(&k["authorPrivateKeyHex"])).unwrap();
    assert_eq!(author.public().did_key(), s(&k["authorDidKey"]));
    let secret = decapsulate(&author, s(&k["keyShare"])).unwrap();
    assert_eq!(secret.to_vec(), hex(&k["sharedSecretHex"]));
    assert_eq!(
        want_mac_header(COMMIT_MAC, s(&k["keyShare"])),
        s(&k["wantMacHeader"])
    );
    assert_eq!(
        parse_want_mac(s(&k["wantMacHeader"]), COMMIT_MAC).unwrap(),
        s(&k["keyShare"])
    );

    // A syncer's side and the author's agree.
    let (share, theirs) = encapsulate(&author.public().did_key()).unwrap();
    assert_eq!(decapsulate(&author, &share).unwrap(), theirs);
}

#[test]
fn reads_space_refs() {
    let uri = "at://did:plc:abc/space/fyi.opensocial.members/self";
    let space = SpaceRef::parse(uri).unwrap();
    assert_eq!(space.authority, "did:plc:abc");
    assert_eq!(space.kind, "fyi.opensocial.members");
    assert_eq!(space.skey, "self");
    assert_eq!(space.to_string(), uri);
    for bad in [
        "did:plc:abc/space/fyi.opensocial.members/self",
        "at://did:plc:abc/space/fyi.opensocial.members",
        "at://did:plc:abc/space/fyi.opensocial.members/self/",
        "at://did:plc:abc/repo/fyi.opensocial.members/self",
        "at://alice.test/space/fyi.opensocial.members/self",
        "at://did:plc:abc/space/members/self",
        "at://did:plc:abc/space/fyi.opensocial.members/..",
    ] {
        assert_eq!(
            SpaceRef::parse(bad).unwrap_err().to_string(),
            format!("not a space: {bad}")
        );
    }
}

#[test]
fn refuses_keys_and_headers_it_cannot_use() {
    let k256 = Keypair::generate(Curve::K256);
    let p256 = Keypair::generate(Curve::P256);
    let refused =
        |r: Result<_, atproto::space::SpaceError>| r.map(|_: [u8; 32]| ()).unwrap_err().to_string();
    assert_eq!(
        refused(decapsulate(&k256, &p256.public().did_key())),
        "a KEM key is P-256"
    );
    assert_eq!(
        refused(decapsulate(&p256, &k256.public().did_key())),
        "a KEM key is a P-256 did:key"
    );
    assert_eq!(
        encapsulate("did:key:z1").unwrap_err().to_string(),
        "a KEM key is a P-256 did:key"
    );
    let share = p256.public().did_key();
    for (header, why) in [
        ("type", "a malformed MAC header"),
        (
            &format!("type=\"{RECORD_MAC}\", ver=1, key_share=\"{share}\"") as &str,
            "a MAC header not of atproto-space-repo, version 1",
        ),
        (
            "type=\"atproto-space-repo\", ver=2",
            "a MAC header not of atproto-space-repo, version 1",
        ),
        (
            "type=\"atproto-space-repo\", ver=1",
            "a MAC header with no key share",
        ),
        (
            "type=\"atproto-space-repo\", ver=1, key_share=\"x\"",
            "a MAC header with no key share",
        ),
    ] {
        assert_eq!(
            parse_want_mac(header, COMMIT_MAC).unwrap_err().to_string(),
            why,
            "{header}"
        );
    }
}

#[test]
fn reads_a_star_archive_back() {
    let v = vectors();
    let c = &v["commit"];
    let star_bytes = hex(&c["starHex"]);
    let (commit, records) = atproto::space::read_star(&star_bytes).unwrap();
    assert_eq!(commit.repo, s(&c["repo"]));
    assert_eq!(commit.space, s(&c["space"]));
    assert_eq!(commit.rev, s(&c["rev"]));
    assert_eq!(commit.data.to_vec(), hex(&c["dataHex"]));
    assert_eq!(star(&commit, &records), star_bytes);
    assert_eq!(records.len(), c["records"].as_array().unwrap().len());

    // An empty repo is its commit alone.
    let (empty, none) = atproto::space::read_star(&star(&commit, &BTreeMap::new())).unwrap();
    assert_eq!((empty, none.len()), (commit.clone(), 0));

    let refused = |bytes: &[u8]| atproto::space::read_star(bytes).unwrap_err().to_string();
    assert_eq!(refused(b"nope"), "not a STAR archive");
    let magic = atproto::space::STAR_MAGIC.to_vec();
    assert_eq!(
        refused(&[magic.clone(), vec![0x05, 1]].concat()),
        "a STAR archive cut short"
    );
    assert_eq!(
        refused(&[magic.clone(), vec![0xff; 6]].concat()),
        "a STAR length too long"
    );
    assert_eq!(
        refused(&[magic.clone(), vec![0x81, 0x00]].concat()),
        "a STAR length not in its shortest form"
    );
    assert_eq!(refused(&magic), "a STAR archive with no metadata");
    assert_eq!(
        refused(&[magic.clone(), vec![0x81]].concat()),
        "a STAR archive cut short"
    );
    assert_eq!(
        refused(&[magic.clone(), vec![1, 0x00]].concat()),
        "STAR metadata that is not a DRISL map"
    );
    let not_a_commit = drisl::encode(&atproto::Value::Map(BTreeMap::from([(
        "$type".into(),
        atproto::Value::String("x".into()),
    )])));
    let framed = |b: &[u8]| [vec![b.len() as u8], b.to_vec()].concat();
    assert_eq!(
        refused(&[magic.clone(), framed(&not_a_commit)].concat()),
        "not a repo's STAR archive"
    );
    let untyped = drisl::encode(&atproto::Value::Map(BTreeMap::new()));
    assert_eq!(
        refused(&[magic.clone(), framed(&untyped)].concat()),
        "STAR metadata with no $type"
    );
    let head = star(&commit, &BTreeMap::new());
    let record = |path: &str| [framed(path.as_bytes()), framed(b"\xa0")].concat();
    assert_eq!(
        refused(&[head.clone(), record("b/1"), record("a/1")].concat()),
        "a STAR archive's records out of order"
    );
    assert_eq!(
        refused(&[head.clone(), record("a/1"), record("a/1")].concat()),
        "a STAR archive's records out of order"
    );
    assert_eq!(
        refused(&[head.clone(), framed(b"\xff"), framed(b"\xa0")].concat()),
        "a STAR key that is not text"
    );
    assert_eq!(
        refused(&[head.clone(), framed(b"a/1")].concat()),
        "a STAR key with no value"
    );
    assert_eq!(
        refused(&[head.clone(), framed(b"a/1"), vec![5, 1]].concat()),
        "a STAR archive cut short"
    );
    // A record is a mebibyte at most.
    let mut big = BTreeMap::new();
    big.insert("a/1".to_string(), vec![0xa0; RECORD_MAX + 1]);
    assert_eq!(
        refused(&star(&commit, &big)),
        format!(
            "a STAR frame of {} bytes, past {RECORD_MAX}",
            RECORD_MAX + 1
        )
    );
}

#[test]
fn writes_and_reads_a_star_archive_of_any_profile() {
    let meta = BTreeMap::from([("$type".into(), atproto::Value::String("test".into()))]);
    let pairs: [(&[u8], &[u8]); 2] = [(&[1], &[2, 3]), (&[], &[])];
    let bytes = write_star(&meta, pairs);
    assert_eq!(bytes[..4], STAR_MAGIC);
    let read = read_star_parts(&bytes, 2).unwrap();
    assert_eq!(
        read,
        Star {
            meta,
            pairs: pairs.to_vec()
        }
    );
    assert!(read_star_parts(&bytes, 1).is_err());
}

#[test]
fn reads_a_space_meta_archive_as_the_alpha_writes_it() {
    let v = reference("space-meta.json");
    let read = |case: &Value| read_space_meta(&hex(&case["hex"])).unwrap();
    let full = read(&v["full"]);
    let want = &v["full"]["read"];
    assert_eq!(full.space, s(&want["space"]));
    let json = |m: &BTreeMap<String, atproto::Value>| atproto::Value::Map(m.clone()).to_json();
    assert_eq!(json(&full.policy), want["policy"]);
    let state: Vec<(String, Value)> = full
        .policy_state
        .iter()
        .map(|(k, v)| (k.clone(), json(v)))
        .collect();
    let wanted: Vec<(String, Value)> = want["policyState"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (s(&e["key"]).to_string(), e["value"].clone()))
        .collect();
    assert_eq!(state, wanted);
    let writers: Vec<SpaceWriter> = want["writers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| SpaceWriter {
            did: s(&w["did"]).into(),
            rev: s(&w["rev"]).into(),
            data: hex(&w["data"]),
            space_rev: s(&w["spaceRev"]).into(),
        })
        .collect();
    assert_eq!(full.writers, writers);

    let empty = read(&v["empty"]);
    assert_eq!(json(&empty.policy), v["empty"]["read"]["policy"]);
    assert!(empty.policy_state.is_empty() && empty.writers.is_empty());

    // A writer's digest is bytes.
    let text = |t: &str| atproto::Value::String(t.into());
    let map = |pairs: &[(&str, atproto::Value)]| {
        let m: BTreeMap<String, atproto::Value> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        drisl::encode(&atproto::Value::Map(m))
    };
    let meta = BTreeMap::from([
        ("$type".into(), text("atproto-space-meta")),
        ("ver".into(), atproto::Value::Integer(1)),
        ("space".into(), text(&full.space)),
    ]);
    let policy = map(&[("$type", text("com.atproto.personalspace.policy"))]);
    let tid = text("3kbcq3p7ad42b");
    let writer = map(&[
        ("rev", tid.clone()),
        ("data", text("01")),
        ("spaceRev", tid),
    ]);
    let archive = write_star(
        &meta,
        [
            (&b"policy"[..], &policy[..]),
            (b"writer/did:plc:alice", &writer),
        ],
    );
    assert_eq!(
        read_space_meta(&archive).unwrap_err().to_string(),
        "writer/did:plc:alice is no writer"
    );

    // Whatever the alpha refuses, refused here too.
    let refused = v["refused"].as_array().unwrap();
    assert_eq!(refused.len(), 19);
    for case in refused {
        let why = read_space_meta(&hex(&case["hex"]));
        assert!(
            why.is_err(),
            "{} read, though the alpha says: {}",
            case["why"],
            case["error"]
        );
    }
}
