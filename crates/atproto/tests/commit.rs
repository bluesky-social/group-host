//! Commits signed, written, read back and checked.

use atproto::cid::Cid;
use atproto::commit::Commit;
use atproto::drisl::{self, Value};
use atproto::keys::{Curve, Keypair};
use std::collections::BTreeMap;

#[test]
fn signs_and_verifies() {
    for curve in [Curve::K256, Curve::P256] {
        let key = Keypair::generate(curve);
        let data = Cid::of_cbor(b"\xa0");
        let commit = Commit::sign("did:plc:abc", data, "3l5mz5mkbn22b", &key);
        assert!(commit.verify(&key.public()));
        assert!(!commit.verify(&Keypair::generate(curve).public()));
        let read = Commit::from_bytes(&commit.to_bytes()).unwrap();
        assert_eq!(read, commit);
        assert_eq!(read.cid(), Cid::of_cbor(&commit.to_bytes()));
        let mut changed = commit.clone();
        changed.rev = "3l5mz5mkbn22c".into();
        assert!(!changed.verify(&key.public()));
    }
}

#[test]
fn keeps_a_prev() {
    let key = Keypair::generate(Curve::K256);
    let mut commit = Commit::sign("did:plc:abc", Cid::of_cbor(b"\xa0"), "3l5mz5mkbn22b", &key);
    commit.prev = Some(Cid::of_cbor(b"\xf6"));
    assert_eq!(Commit::from_bytes(&commit.to_bytes()).unwrap(), commit);
}

#[test]
fn refuses_what_is_no_commit() {
    let refused = |v: Value| {
        Commit::from_bytes(&drisl::encode(&v))
            .unwrap_err()
            .to_string()
    };
    assert_eq!(refused(Value::Integer(1)), "not a version 3 commit");
    assert_eq!(
        Commit::from_bytes(&[0xff]).unwrap_err().to_string(),
        "not a version 3 commit"
    );
    let key = Keypair::generate(Curve::K256);
    let good = Commit::sign("did:plc:abc", Cid::of_cbor(b"\xa0"), "3l5mz5mkbn22b", &key);
    let Value::Map(map) = drisl::decode(&good.to_bytes()).unwrap() else {
        unreachable!()
    };
    let with = |k: &str, v: Value| {
        let mut m: BTreeMap<String, Value> = map.clone();
        m.insert(k.into(), v);
        Value::Map(m)
    };
    assert_eq!(
        refused(with("version", Value::Integer(2))),
        "not a version 3 commit"
    );
    assert_eq!(
        refused(with("prev", Value::Integer(2))),
        "not a version 3 commit"
    );
}
