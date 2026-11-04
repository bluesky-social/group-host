//! JWKs, thumbprints and JWTs, against what jose makes of the same keys
//! (tests/reference/jose.json) and RFC 9449's example.

use atproto::jose::{self, from_jwk, jwk, parts, random, sha256, sign, thumbprint, verify};
use atproto::keys::{Curve, Keypair};
use serde_json::{Value, json};

fn vectors() -> Value {
    let path = format!("{}/tests/reference/jose.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn makes_the_jwk_and_thumbprint_jose_makes() {
    for v in vectors()["keys"].as_array().unwrap() {
        let curve = if v["alg"] == "ES256" {
            Curve::P256
        } else {
            Curve::K256
        };
        let key = Keypair::from_bytes(curve, &unhex(v["secret"].as_str().unwrap())).unwrap();
        assert_eq!(jwk(&key.public()), v["jwk"], "{}", v["crv"]);
        assert_eq!(thumbprint(&key.public()), v["thumbprint"], "{}", v["crv"]);
        assert_eq!(from_jwk(&v["jwk"]).unwrap(), key.public());
    }
}

#[test]
fn takes_the_thumbprint_rfc_9449_gives() {
    let rfc = &vectors()["rfc9449"];
    let key = from_jwk(&rfc["jwk"]).unwrap();
    assert_eq!(thumbprint(&key), rfc["thumbprint"]);
}

#[test]
fn checks_the_proofs_jose_signed() {
    for v in vectors()["keys"].as_array().unwrap() {
        let token = v["jwt"].as_str().unwrap();
        let (header, _) = parts(token).unwrap();
        let key = from_jwk(&header["jwk"]).unwrap();
        let (header, claims) = verify(token, &key).unwrap();
        assert_eq!(header["typ"], "dpop+jwt");
        assert_eq!(claims["htu"], "https://host.test/oauth/token");
    }
}

#[test]
fn signs_what_it_checks() {
    for curve in [Curve::P256, Curve::K256] {
        let key = Keypair::generate(curve);
        let token = sign(&key, &json!({"typ": "dpop+jwt"}), &json!({"jti": "a"}));
        let (header, claims) = verify(&token, &key.public()).unwrap();
        assert_eq!(header["alg"], curve.alg());
        assert_eq!(claims["jti"], "a");
    }
}

#[test]
fn refuses_another_key_or_alg() {
    let key = Keypair::generate(Curve::P256);
    let token = sign(&key, &json!({}), &json!({}));
    let other = Keypair::generate(Curve::P256).public();
    assert_eq!(
        verify(&token, &other).unwrap_err().to_string(),
        "bad signature"
    );
    let k256 = Keypair::generate(Curve::K256).public();
    let said = verify(&token, &k256).unwrap_err().to_string();
    assert!(said.contains("where ES256K was expected"), "{said}");
}

#[test]
fn refuses_what_is_not_a_jwt() {
    for token in ["a.b", "a.b.c.d", "!!.e30.x", "e30.WzFd.x"] {
        assert!(parts(token).is_err(), "{token}");
    }
}

#[test]
fn refuses_a_jwk_it_cannot_use() {
    let good = vectors()["keys"][0]["jwk"].clone();
    let with = |k: &str, v: Value| {
        let mut j = good.clone();
        j[k] = v;
        from_jwk(&j).unwrap_err().0
    };
    assert!(with("d", json!("secret")).contains("private"));
    assert!(with("kty", json!("RSA")).contains("not an EC key"));
    assert!(with("crv", json!("P-384")).contains("curve"));
    assert!(with("x", json!("short")).contains("x is not 32 bytes"));
    // A point not on the curve.
    assert!(with("y", json!(jose::b64([7u8; 32]))).contains("P-256"));
}

#[test]
fn hashes_and_draws_as_oauth_wants() {
    // RFC 7636's example verifier and its S256 challenge.
    assert_eq!(
        sha256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    assert_eq!(random(32).len(), 43);
    assert_eq!(jose::unb64(&jose::b64(b"bytes")).unwrap(), b"bytes");
    assert_eq!(jose::unb64("!!"), None);
    assert_ne!(random(16), random(16));
}
