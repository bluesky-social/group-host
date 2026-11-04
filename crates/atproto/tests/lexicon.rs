//! Values checked against lexicons, against the verdicts of atproto's
//! reference (tests/reference/lexicon.json, from lexicon.mjs), and the
//! string formats against atproto's interop vectors (tests/interop/syntax).

use atproto::lexicon::{
    Lexicons, is_at_identifier, is_at_uri, is_cid_string, is_datetime, is_format, is_language,
    is_uri,
};
use serde_json::{Value, json};

/// The cases this departs from the reference on, and its own verdict on
/// each: the module's doc comment says why.
const DEPARTURES: [(&str, bool); 7] = [
    // Permissioned spaces' URIs, which the reference does not know yet.
    ("at-uri-valid-space-ref", true),
    ("at-uri-valid-space-ref-self", true),
    ("at-uri-valid-space-record", true),
    // A blob's accept and maxSize, which the reference does not check.
    ("blob-data-invalid-unaccepted-mime", false),
    ("blob-data-invalid-exceeds-maxSize", false),
    ("e2e-profile-invalid-avatar-wrong-mime", false),
    // A version 0 CID, which the cid format's interop vectors refuse.
    ("cid-invalid-qmb-prefix", false),
];

fn vectors() -> Vec<Value> {
    let path = format!(
        "{}/tests/reference/lexicon.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let file: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    file["vectors"].as_array().unwrap().clone()
}

fn check(v: &Value) -> Result<(), atproto::lexicon::LexiconError> {
    let lexicons = Lexicons::new(v["lexicons"].as_array().unwrap().iter().cloned());
    let reference = v["ref"].as_str().unwrap();
    match v["kind"].as_str().unwrap() {
        "params" => lexicons.validate_params(reference, v["value"].as_object().unwrap()),
        "input" => lexicons.validate_input(reference, &v["value"]),
        _ => lexicons.validate(reference, &v["value"]),
    }
}

#[test]
fn every_vector() {
    let vectors = vectors();
    assert!(vectors.len() > 200);
    let mut departed = 0;
    for v in &vectors {
        let name = v["name"].as_str().unwrap();
        let ours = check(v);
        let want = match DEPARTURES.iter().find(|(n, _)| *n == name) {
            Some((_, verdict)) => {
                departed += 1;
                assert_ne!(*verdict, v["valid"].as_bool().unwrap(), "{name} departs");
                *verdict
            }
            None => v["valid"].as_bool().unwrap(),
        };
        assert_eq!(
            ours.is_ok(),
            want,
            "{name}: {ours:?}, the reference: {}",
            v["error"]
        );
    }
    assert_eq!(departed, DEPARTURES.len());
}

/// The lines of an interop vector file, less comments and blank lines.
fn lines(name: &str) -> Vec<String> {
    let path = format!("{}/tests/interop/syntax/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

fn interop(kind: &str, is: fn(&str) -> bool) {
    for valid in lines(&format!("{kind}_syntax_valid.txt")) {
        assert!(is(&valid), "{kind} should be valid: {valid:?}");
    }
    for invalid in lines(&format!("{kind}_syntax_invalid.txt")) {
        assert!(!is(&invalid), "{kind} should be invalid: {invalid:?}");
    }
}

#[test]
fn formats_against_interop() {
    interop("aturi", is_at_uri);
    interop("atidentifier", is_at_identifier);
    interop("cid", is_cid_string);
    interop("datetime", is_datetime);
    interop("language", is_language);
    interop("uri", is_uri);
    for invalid in lines("datetime_parse_invalid.txt") {
        assert!(!is_datetime(&invalid), "{invalid:?}");
    }
}

#[test]
fn formats() {
    for (format, s) in [
        ("did", "did:plc:abc"),
        ("handle", "a.example.com"),
        ("nsid", "fyi.opensocial.role"),
        ("tid", "3jzfcijpj2z2a"),
        ("record-key", "self"),
        ("a-format-to-come", " anything "),
        ("at-uri", "at://did:plc:abc/fyi.opensocial.role/x#/a%C3%A9"),
        ("datetime", "1985-04-31T23:20:50Z"),
        ("datetime", "9999-12-31T23:59:59+01:00"),
    ] {
        assert!(is_format(format, s), "{format} {s}");
    }
    for (format, s) in [
        ("at-uri", "at://did:plc:abc/fyi.opensocial.role/x#/%FF"),
        ("at-uri", "at://did:plc:abc/fyi.opensocial.role/x#/%G1"),
        ("at-uri", "at://did:plc:abc/fyi.opensocial.role/x?q"),
        (
            "at-uri",
            "at://alice.example.com/space/fyi.opensocial.space/x",
        ),
        (
            "at-uri",
            "at://did:plc:abc/space/fyi.opensocial.space/x/did:plc:w/nsid/r",
        ),
        ("datetime", "9999-12-31T23:59:59-01:00"),
        ("datetime", "1985-04-12T23:20:50.123+24:00"),
        ("datetime", "1985-04-12T23:20:50x"),
        ("datetime", "1985-04-12T23:20:50.Z"),
        ("datetime", "1985-04-12T23:20:50é"),
        ("language", "en-a"),
        ("language", "de-1901-1901"),
        ("language", "x"),
    ] {
        assert!(!is_format(format, s), "{format} {s}");
    }
}

fn host() -> Lexicons {
    Lexicons::new([
        json!({"lexicon": 1, "id": "test.call", "defs": {
            "main": {"type": "query", "parameters": {"type": "params", "required": ["n"], "properties": {
                "n": {"type": "integer"},
                "b": {"type": "boolean"},
                "s": {"type": "string", "default": "x"},
                "tags": {"type": "array", "items": {"type": "integer"}}
            }}}
        }}),
        json!({"lexicon": 1, "id": "test.bare", "defs": {"main": {"type": "procedure"}}}),
        json!({"lexicon": 1, "defs": {}}),
    ])
}

#[test]
fn query_strings_are_read_as_their_types() {
    let lex = host();
    let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
        p.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let params = lex.query_params(
        "test.call",
        &pairs(&[
            ("n", "3"),
            ("b", "true"),
            ("tags", "1"),
            ("tags", "2"),
            ("other", "x"),
        ]),
    );
    assert_eq!(
        Value::Object(params.clone()),
        json!({"n": 3, "b": true, "tags": [1, 2]})
    );
    assert!(lex.validate_params("test.call", &params).is_ok());
    let params = lex.query_params("test.call", &pairs(&[("n", "three")]));
    let err = lex.validate_params("test.call", &params).unwrap_err();
    assert_eq!(err.to_string(), "params.n must be an integer");
    let err = lex
        .validate_params("test.call", &Default::default())
        .unwrap_err();
    assert_eq!(err.to_string(), "params must have the property \"n\"");
    assert!(
        lex.query_params("test.none", &pairs(&[("n", "1")]))
            .is_empty()
    );
    assert!(
        lex.validate_params("test.bare", &Default::default())
            .is_ok()
    );
    assert!(lex.validate_input("test.bare", &json!({})).is_ok());
    let err = lex.validate_input("test.call", &json!({})).unwrap_err();
    assert_eq!(err.to_string(), "input test.call is not a procedure");
    let err = lex.validate("test.none", &json!({})).unwrap_err();
    assert_eq!(err.to_string(), "record no lexicon test.none");
    assert!(lex.def("test.call#main").is_some());
}

#[test]
fn errors_say_where() {
    let v = vectors();
    let path = v
        .iter()
        .find(|v| v["name"] == "own-invalid-input-path")
        .unwrap();
    let err = check(path).unwrap_err();
    assert_eq!(err.path, "input.roles[2].role");
    assert_eq!(err.message, "must not be longer than 3 bytes");
}

#[test]
fn edges() {
    let lex = Lexicons::new([json!({"lexicon": 1, "id": "test.edge", "defs": {
        "main": {"type": "record", "key": "tid", "record": {"type": "object",
            "required": ["flag", "inner"],
            "properties": {
                "flag": {"type": "boolean", "default": true},
                "inner": {"type": "ref", "ref": "#inner"},
                "list": {"type": "array", "items": {"type": "unknown"}},
                "link": {"type": "cid-link"},
                "loose": {"type": "unknown"}
            }}},
        "inner": {"type": "object"}
    }})]);
    let ok = |v: Value| lex.validate("test.edge", &v);
    assert!(ok(json!({"inner": {}})).is_ok());
    let missing = ok(json!({})).unwrap_err();
    assert_eq!(
        missing.to_string(),
        "record must have the property \"inner\""
    );
    let inner = ok(json!({"inner": 1})).unwrap_err();
    assert_eq!(inner.to_string(), "record.inner must be an object");
    let list = ok(json!({"inner": {}, "list": {}})).unwrap_err();
    assert_eq!(list.to_string(), "record.list must be an array");
    let bytes = ok(json!({"inner": {}, "loose": {"a": [{"$bytes": "!"}]}})).unwrap_err();
    assert_eq!(
        bytes.to_string(),
        "record.loose.a[0] must hold base64 in $bytes"
    );
    let cid = |bytes: &[u8]| {
        format!(
            "b{}",
            atproto::base32::encode(atproto::base32::LOWER, bytes)
        )
    };
    let raw = atproto::Cid::of_raw(b"x").to_bytes();
    let z = format!("z{}", bs58::encode(&raw).into_string());
    for (link, valid) in [
        (z, true),
        (cid(&raw), true),
        (cid(&[2, 0x55, 0x12, 0]), false),
        (cid(&[0x80; 10]), false),
        ("xyz".to_string(), false),
    ] {
        let v = ok(json!({"inner": {}, "link": {"$link": link}}));
        assert_eq!(v.is_ok(), valid, "{link}");
    }
    let params = Lexicons::new([json!({"lexicon": 1, "id": "test.q", "defs": {"main": {
        "type": "query", "parameters": {"type": "params", "properties": {"s": {"type": "string"}}}
    }}})]);
    let given = params.query_params("test.q", &[("s".into(), "1".into())]);
    assert_eq!(Value::Object(given), json!({"s": "1"}));
    assert!(is_language("zh-yue-HK"));
}
