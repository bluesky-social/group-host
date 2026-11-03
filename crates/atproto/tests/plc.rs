//! PLC operations, against what @did-plc/lib makes of the same keys
//! (tests/reference/identity.json): the same bytes, signatures and DIDs.

use atproto::keys::{Curve, Keypair};
use atproto::plc::{Op, Service, Signed, tombstone};
use serde_json::{Value, json};

fn reference() -> Value {
    let path = format!(
        "{}/tests/reference/identity.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn key(curve: Curve, hex: &Value) -> Keypair {
    let h = hex.as_str().unwrap();
    let bytes: Vec<u8> = (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect();
    Keypair::from_bytes(curve, &bytes).unwrap()
}

#[test]
fn operations_as_the_reference_makes_them() {
    let r = reference();
    let rotation = key(Curve::K256, &r["keys"]["rotation"]);
    let signing = key(Curve::K256, &r["keys"]["signing"]);
    let p256 = key(Curve::P256, &r["keys"]["p256Rotation"]);
    for (v, signer, keys) in [
        (&r["plc"][0], &rotation, vec![rotation.public().did_key()]),
        (
            &r["plc"][1],
            &p256,
            vec![p256.public().did_key(), rotation.public().did_key()],
        ),
    ] {
        let op = Op::atproto(
            &signing.public().did_key(),
            &keys,
            "riders.groups.example.com",
            "https://groups.example.com",
        );
        let genesis = op.clone().sign(None, signer);
        assert_eq!(genesis.to_json(), v["genesis"], "{}", v["name"]);
        assert_eq!(genesis.did(), v["did"].as_str().unwrap());
        assert_eq!(genesis.cid().to_string(), v["genesisCid"].as_str().unwrap());
        assert_eq!(genesis.prev(), None);
        assert_eq!(genesis.signed_by(&keys), Some(0));

        let mut handle = op.clone();
        handle.also_known_as = vec!["at://riders.example.org".into()];
        assert_eq!(handle.handle(), Some("riders.example.org"));
        let handle = handle.sign(Some(genesis.cid()), signer);
        assert_eq!(handle.to_json(), v["updateHandle"]);

        let mut pds = op.clone();
        pds.also_known_as = vec!["at://riders.example.org".into()];
        pds.services.insert(
            "atproto_pds".into(),
            Service {
                kind: "AtprotoPersonalDataServer".into(),
                endpoint: "https://other.example.com".into(),
            },
        );
        let pds = pds.sign(Some(handle.cid()), signer);
        assert_eq!(pds.to_json(), v["updatePds"]);

        let end = tombstone(pds.cid(), signer);
        assert_eq!(end.to_json(), v["tombstone"]);
        assert_eq!(end.prev(), Some(pds.cid()));
        assert_eq!(end.signed_by(&keys), Some(0));

        for (json, ours) in [
            (&v["genesis"], &genesis),
            (&v["updatePds"], &pds),
            (&v["tombstone"], &end),
        ] {
            assert_eq!(&Signed::from_json(json).unwrap(), ours);
        }
    }
}

#[test]
fn says_which_key_signed() {
    let r = reference();
    let rotation = key(Curve::K256, &r["keys"]["rotation"]);
    let other = Keypair::generate(Curve::P256);
    let genesis = Signed::from_json(&r["plc"][0]["genesis"]).unwrap();
    let keys = [
        other.public().did_key(),
        "did:key:z1".into(),
        rotation.public().did_key(),
    ];
    assert_eq!(genesis.signed_by(&keys), Some(2));
    assert_eq!(genesis.signed_by(&keys[..2]), None);
    assert_eq!(genesis.signed_by(&[]), None);
}

#[test]
fn refuses_what_is_no_operation() {
    let r = reference();
    let g = &r["plc"][0]["genesis"];
    let with = |k: &str, v: Value| {
        let mut j = g.clone();
        j[k] = v;
        Signed::from_json(&j).unwrap_err().to_string()
    };
    assert_eq!(with("sig", json!("!")), "an operation's sig is base64url");
    assert_eq!(
        with("prev", json!("x")),
        "prev: a CID is base32, starting with b"
    );
    assert_eq!(
        with("rotationKeys", json!([1])),
        "rotationKeys is a list of text"
    );
    assert_eq!(
        with("alsoKnownAs", json!("x")),
        "alsoKnownAs is a list of text"
    );
    assert_eq!(
        with("verificationMethods", json!({"atproto": 1})),
        "an operation's keys and services are maps of text"
    );
    assert_eq!(
        with("services", json!({"atproto_pds": {"type": 1}})),
        "an operation's keys and services are maps of text"
    );
    assert_eq!(
        with("services", json!([])),
        "an operation's keys and services are maps of text"
    );
    assert_eq!(
        with("type", json!("create")),
        "not a plc_operation or plc_tombstone"
    );
    let mut t = r["plc"][0]["tombstone"].clone();
    t["prev"] = Value::Null;
    assert_eq!(
        Signed::from_json(&t).unwrap_err().to_string(),
        "a tombstone names the operation it ends"
    );
}
