//! The space alpha's primitives, against vectors made by running
//! `@atproto/space` (tests/reference/space-vectors.json, from pds.js).

use atproto::cid::Cid;
use atproto::keys::{Curve, Keypair};
use atproto::space::{
    COMMIT_MAC, Commit, LtHash, RECORD_MAC, STATE_BYTES, SpaceRef, decapsulate, element,
    encapsulate, mac_header, parse_want_mac, record_mac, star, want_mac_header,
};
use serde_json::Value;
use std::collections::BTreeMap;

fn vectors() -> Value {
    let path = format!(
        "{}/tests/reference/space-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
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
