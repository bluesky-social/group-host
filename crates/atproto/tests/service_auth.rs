//! Service-auth JWTs: one the reference PDS signs
//! (tests/reference/identity.json), one knotless signs
//! (tests/reference/knotless.json), and our own, checked as knotless
//! checks them.

use atproto::keys::{Curve, Keypair, PublicKey};
use atproto::service_auth::{Claims, check, parts, sign, verify};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

fn read(name: &str) -> Value {
    let path = format!("{}/tests/reference/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

const NOW: u64 = 1_793_736_300;

#[test]
fn checks_what_others_sign() {
    let r = read("identity.json");
    let jwt = &r["serviceJwt"];
    let key = PublicKey::from_did_key(jwt["signingDidKey"].as_str().unwrap()).unwrap();
    let claims = verify(jwt["token"].as_str().unwrap(), &key, NOW).unwrap();
    assert_eq!(
        check(
            &claims,
            "did:web:groups.example.com",
            "fyi.opensocial.getGroup"
        )
        .unwrap(),
        jwt["iss"].as_str().unwrap()
    );
    assert!(claims.jti.is_some() && claims.iat.is_some());

    let k = read("knotless.json");
    let key = PublicKey::from_did_key(k["didKey"].as_str().unwrap()).unwrap();
    let claims = verify(k["token"].as_str().unwrap(), &key, NOW).unwrap();
    assert_eq!(
        check(
            &claims,
            "did:web:groups.example.com",
            "fyi.opensocial.getGroup"
        )
        .unwrap(),
        "did:plc:abc"
    );
}

fn claims() -> Claims {
    Claims {
        iss: "did:web:labeler.example.com#atproto_labeler".into(),
        aud: "did:web:groups.example.com".into(),
        exp: NOW + 60,
        iat: None,
        lxm: Some("fyi.opensocial.getGroup".into()),
        jti: None,
    }
}

#[test]
fn signs_and_checks_its_own() {
    for curve in [Curve::K256, Curve::P256] {
        let key = Keypair::generate(curve);
        let token = sign(&key, &claims());
        let read = verify(&token, &key.public(), NOW).unwrap();
        assert_eq!(read, claims());
        assert_eq!(
            check(
                &read,
                "did:web:groups.example.com",
                "fyi.opensocial.getGroup"
            )
            .unwrap(),
            "did:web:labeler.example.com"
        );
        let (header, _) = parts(&token).unwrap();
        assert_eq!(header["alg"], curve.alg());
    }
}

#[test]
fn refuses_what_does_not_hold() {
    let key = Keypair::generate(Curve::K256);
    let token = sign(&key, &claims());
    let refused = |t: &str, k: &PublicKey, now: u64| verify(t, k, now).unwrap_err().to_string();
    let other = Keypair::generate(Curve::P256).public();
    assert_eq!(
        refused(&token, &other, NOW),
        r#"a JWT signed with "ES256K" where ES256 was expected"#
    );
    assert_eq!(
        refused(&token, &Keypair::generate(Curve::K256).public(), NOW),
        "bad signature"
    );
    assert_eq!(refused(&token, &key.public(), NOW + 60), "an expired JWT");
    assert_eq!(
        refused("!.x.y", &key.public(), NOW),
        "bad base64 in a JWT header"
    );
    let b = |s: &str| URL_SAFE_NO_PAD.encode(s);
    assert_eq!(
        refused(&format!("{}.x", b("{")), &key.public(), NOW),
        "bad JSON in a JWT header"
    );
    assert_eq!(
        refused(&b("{}"), &key.public(), NOW),
        "bad JSON in a JWT payload"
    );
    assert_eq!(
        refused(&format!("{}.{}.x", b("{}"), b("{}")), &key.public(), NOW),
        "a JWT without iss, aud and exp"
    );

    let read = verify(&token, &key.public(), NOW).unwrap();
    assert_eq!(
        check(&read, "did:web:other", "fyi.opensocial.getGroup")
            .unwrap_err()
            .to_string(),
        "a token for did:web:groups.example.com, not did:web:other"
    );
    assert_eq!(
        check(&read, "did:web:groups.example.com", "x.y.z")
            .unwrap_err()
            .to_string(),
        r#"a token for Some("fyi.opensocial.getGroup"), not x.y.z"#
    );
}
