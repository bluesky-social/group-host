//! DRISL, against atproto's interop test vectors (tests/interop/data-model),
//! and every form it refuses.

use atproto::drisl::{self, MAX_DEPTH, Value, check_record, decode, encode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use serde_json::json;

fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!(
        "{}/tests/interop/data-model/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn encodes_as_the_vectors_say() {
    for v in vectors("data-model-fixtures.json") {
        let value = Value::from_json(&v["json"]).unwrap();
        let cbor = STANDARD_NO_PAD
            .decode(v["cbor_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(encode(&value), cbor);
        assert_eq!(drisl::cid(&value).to_string(), v["cid"].as_str().unwrap());
        let back = decode(&cbor).unwrap();
        assert_eq!(back, value);
        assert_eq!(back.to_json(), v["json"]);
    }
}

#[test]
fn records_as_the_vectors_say() {
    for v in vectors("data-model-valid.json") {
        assert!(check_record(&v["json"]).is_ok(), "{}", v["note"]);
    }
    for v in vectors("data-model-invalid.json") {
        assert!(check_record(&v["json"]).is_err(), "{}", v["note"]);
    }
}

#[test]
fn round_trips_every_kind_and_size() {
    let value = Value::from_json(&json!({
        "n": [0, 23, 24, 255, 256, 65535, 65536, 4294967295u64, 4294967296u64, i64::MAX],
        "neg": [-1, -24, -25, -256, -257, -65537, -4294967297i64, i64::MIN],
        "f": false, "t": true, "z": null,
        "b": {"$bytes": "AAEC"},
        "padded": {"$bytes": "AAE="},
        "s": "x".repeat(300),
    }))
    .unwrap();
    assert_eq!(decode(&encode(&value)).unwrap(), value);
    assert_eq!(value.to_json()["padded"], json!({"$bytes": "AAE"}));
    assert_eq!(value.to_json()["z"], json!(null));
    assert_eq!(value.to_json()["f"], json!(false));
}

fn refused(bytes: &[u8]) -> String {
    decode(bytes).unwrap_err().to_string()
}

#[test]
fn refuses_every_other_form() {
    assert_eq!(refused(&[0x01, 0x02]), "bytes after the value");
    assert_eq!(
        refused(&[0x62, b'a']),
        "the bytes end in the middle of a value"
    );
    assert_eq!(refused(&[0x9f]), "an indefinite or reserved length");
    assert_eq!(
        refused(&[0x18, 0x05]),
        "a number written longer than it needs"
    );
    assert_eq!(
        refused(&[0x19, 0x00, 0x05]),
        "a number written longer than it needs"
    );
    assert_eq!(
        refused(&[0x1a, 0, 0, 0, 5]),
        "a number written longer than it needs"
    );
    assert_eq!(
        refused(&[0x1b, 0, 0, 0, 0, 0, 0, 0, 5]),
        "a number written longer than it needs"
    );
    assert_eq!(
        refused(&[0x1b, 0xff, 0, 0, 0, 0, 0, 0, 0]),
        "an integer past 64 bits"
    );
    assert_eq!(
        refused(&[0x3b, 0xff, 0, 0, 0, 0, 0, 0, 0]),
        "an integer past 64 bits"
    );
    assert_eq!(refused(&[0x61, 0xff]), "text that is not UTF-8");
    assert_eq!(refused(&[0xa1, 0x01, 0x01]), "a map key that is not text");
    assert_eq!(
        refused(&[0xa2, 0x62, b'a', b'a', 0x01, 0x61, b'b', 0x01]),
        "map keys out of order, or repeated"
    );
    assert_eq!(
        refused(&[0xa2, 0x61, b'a', 0x01, 0x61, b'a', 0x01]),
        "map keys out of order, or repeated"
    );
    assert_eq!(refused(&[0xc1, 0x01]), "tag 1: only links (42) are allowed");
    assert_eq!(
        refused(&[0xd8, 0x2a, 0x61, b'a']),
        "a link that is not bytes starting with 0"
    );
    assert_eq!(
        refused(&[0xd8, 0x2a, 0x41, 0x01]),
        "a link that is not bytes starting with 0"
    );
    assert_eq!(
        refused(&[0xd8, 0x2a, 0x42, 0x00, 0x01]),
        "a link: a CID is version 1 and SHA-256, of DAG-CBOR or raw"
    );
    assert_eq!(refused(&[0xf9, 0x3c, 0x00]), "a float");
    assert_eq!(refused(&[0xf7]), "simple value 23");
    let deep = [vec![0x81; MAX_DEPTH + 1], vec![0x01]].concat();
    assert_eq!(refused(&deep), "values nested too deep");
    assert!(decode(&[vec![0x81; MAX_DEPTH], vec![0x01]].concat()).is_ok());
}

#[test]
fn says_whether_a_value_is_a_map() {
    assert_eq!(
        Value::Map(Default::default()).into_map(),
        Some(Default::default())
    );
    assert_eq!(Value::Null.into_map(), None);
    assert_eq!(Value::Bytes(vec![1]).into_bytes(), Some(vec![1]));
    assert_eq!(Value::Null.into_bytes(), None);
}

#[test]
fn reads_values_one_after_another() {
    let frame = [
        encode(&Value::Integer(1)),
        encode(&Value::String("t".into())),
    ]
    .concat();
    let (first, read) = drisl::decode_first(&frame).unwrap();
    assert_eq!((first, read), (Value::Integer(1), 1));
    assert_eq!(decode(&frame[read..]).unwrap(), Value::String("t".into()));
}

#[test]
fn refuses_json_no_record_holds() {
    let refused = |j: serde_json::Value| Value::from_json(&j).unwrap_err().to_string();
    assert_eq!(refused(json!(1.5)), "1.5 is not an integer");
    assert_eq!(refused(json!(1e300)), "1e+300 is not an integer");
    assert_eq!(
        refused(json!({"$link": 1})),
        "a $link is a CID's text, alone in its object"
    );
    assert_eq!(
        refused(json!({"$link": "."})),
        "$link: a CID is base32, starting with b"
    );
    assert_eq!(
        refused(json!({"$bytes": 1})),
        "$bytes is base64 text, alone in its object"
    );
    assert_eq!(refused(json!({"$bytes": "!"})), "$bytes is not base64");
    assert_eq!(refused(json!([{"a": 0.5}])), "0.5 is not an integer");
    assert_eq!(Value::from_json(&json!(2.0)).unwrap(), Value::Integer(2));
}

#[test]
fn says_why_a_record_is_refused() {
    let why = |j: serde_json::Value| check_record(&j).unwrap_err().to_string();
    assert_eq!(why(json!([])), "a record is an object");
    assert_eq!(why(json!({"a": 0.5})), "0.5 is not an integer");
    assert_eq!(why(json!({"a": [{"$type": ""}]})), "a $type is a name");
    assert_eq!(
        why(json!({"$type": "blob", "size": 1})),
        "a blob has a ref, a mimeType and a size"
    );
    assert!(check_record(&json!({"$type": "x.y.z", "a": [1, "b"]})).is_ok());
}
