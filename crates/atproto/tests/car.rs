//! CAR files written and read, and every file refused.

use atproto::car::{self, Car};
use atproto::cid::Cid;
use atproto::drisl::{self, Value};
use std::collections::BTreeMap;

fn header(value: Value) -> Vec<u8> {
    let h = drisl::encode(&value);
    [vec![h.len() as u8], h].concat()
}

fn roots(roots: Value, version: i64) -> Value {
    Value::Map(BTreeMap::from([
        ("roots".into(), roots),
        ("version".into(), Value::Integer(version)),
    ]))
}

#[test]
fn round_trips() {
    let record = drisl::encode(&Value::String("hello".into()));
    let blob = vec![7u8; 200];
    let (rc, bc) = (Cid::of_cbor(&record), Cid::of_raw(&blob));
    let bytes = car::write(&[rc], [(&rc, &record[..]), (&bc, &blob[..])]);
    let read = car::read(&bytes).unwrap();
    assert_eq!(
        read,
        Car {
            roots: vec![rc],
            blocks: BTreeMap::from([(rc, record), (bc, blob)]),
        }
    );
}

fn refused(bytes: &[u8]) -> String {
    car::read(bytes).unwrap_err().to_string()
}

#[test]
fn refuses_what_is_no_car() {
    let ok = header(roots(Value::Array(vec![]), 1));
    assert_eq!(refused(&[]), "the file ends in the middle of a length");
    assert_eq!(refused(&[0xff; 10]), "a length too long");
    assert_eq!(
        refused(&[5, 0xa0]),
        "the file ends in the middle of a block"
    );
    assert!(refused(&[1, 0xff]).starts_with("the header: "));
    assert_eq!(
        refused(&header(Value::Integer(1))),
        "the header is not a map"
    );
    assert_eq!(
        refused(&header(roots(Value::Array(vec![]), 2))),
        "the header is not version 1 with roots"
    );
    assert_eq!(
        refused(&header(roots(Value::Array(vec![Value::Integer(1)]), 1))),
        "a root is not a link"
    );
    assert_eq!(
        refused(&[ok.clone(), vec![3, 1, 2, 3]].concat()),
        "a block too short for its CID"
    );
    let mut unblessed = Cid::of_raw(b"x").to_bytes();
    unblessed[2] = 0x13;
    assert!(
        refused(&[ok.clone(), vec![unblessed.len() as u8], unblessed].concat())
            .starts_with("a block's CID: ")
    );
    let cid = Cid::of_raw(b"x");
    let lying = [ok.clone(), vec![37], cid.to_bytes(), b"y".to_vec()].concat();
    assert_eq!(
        refused(&lying),
        format!("block {cid} does not hash to its CID")
    );
    let cbor = Cid::of_cbor(b"\xa0");
    let lying = [ok, vec![37], cbor.to_bytes(), b"\xa1".to_vec()].concat();
    assert_eq!(
        refused(&lying),
        format!("block {cbor} does not hash to its CID")
    );
}
