//! Keys and signatures, against atproto's interop test vectors
//! (tests/interop/crypto).

use atproto::keys::{Curve, KeyError, Keypair, PublicKey};
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use serde_json::Value;

fn vectors(name: &str) -> Vec<Value> {
    let path = format!("{}/tests/interop/crypto/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn b64(v: &Value) -> Vec<u8> {
    STANDARD_NO_PAD.decode(v.as_str().unwrap()).unwrap()
}

#[test]
fn signatures_as_the_vectors_say() {
    for v in vectors("signature-fixtures.json") {
        let key = PublicKey::from_did_key(v["publicKeyDid"].as_str().unwrap()).unwrap();
        let legacy = PublicKey::from_method(
            v["didDocSuite"].as_str().unwrap(),
            v["publicKeyMultibase"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(key, legacy);
        assert_eq!(key.curve().alg(), v["algorithm"].as_str().unwrap());
        assert_eq!(key.did_key(), v["publicKeyDid"].as_str().unwrap());
        let valid = key.verify(&b64(&v["messageBase64"]), &b64(&v["signatureBase64"]));
        // A high-S signature is refused here, and taken where either S is.
        let tags = v["tags"].as_array().unwrap();
        if tags.iter().any(|t| t == "high-s") {
            assert!(key.verify_either_s(&b64(&v["messageBase64"]), &b64(&v["signatureBase64"])));
        }
        assert_eq!(
            valid,
            v["validSignature"].as_bool().unwrap(),
            "{}",
            v["comment"]
        );
    }
}

#[test]
fn did_keys_as_the_vectors_say() {
    for (file, curve) in [
        ("w3c_didkey_K256.json", Curve::K256),
        ("w3c_didkey_P256.json", Curve::P256),
    ] {
        for v in vectors(file) {
            // The K-256 vectors give the secret in hex, the P-256 in base58.
            let secret = match v.get("privateKeyBytesHex") {
                Some(h) => hex(h.as_str().unwrap()),
                None => bs58::decode(v["privateKeyBytesBase58"].as_str().unwrap())
                    .into_vec()
                    .unwrap(),
            };
            let key = Keypair::from_bytes(curve, &secret).unwrap();
            assert_eq!(key.public().did_key(), v["publicDidKey"].as_str().unwrap());
            assert_eq!(key.to_bytes().to_vec(), secret);
            assert_eq!(key.curve(), curve);
        }
    }
}

#[test]
fn signs_low_s_and_verifies() {
    for curve in [Curve::K256, Curve::P256] {
        let key = Keypair::generate(curve);
        let public = key.public();
        assert_eq!(format!("{key:?}"), format!("Keypair({})", public.did_key()));
        // Enough signatures that one would come out high-S unless normalized.
        for i in 0..32u8 {
            let sig = key.sign(&[i]);
            assert!(public.verify(&[i], &sig));
            assert!(!public.verify(&[i, i], &sig));
        }
        assert!(!public.verify(b"m", &[0; 10]));
        let again = PublicKey::from_multibase(&public.to_multibase()).unwrap();
        assert_eq!(again, public);
        assert_eq!(
            PublicKey::from_sec1(curve, &public.to_sec1()).unwrap(),
            public
        );
        let method = PublicKey::from_method("Multikey", &public.to_multibase()).unwrap();
        assert_eq!(method, public);
    }
    assert_eq!(Curve::P256.alg(), "ES256");
}

#[test]
fn refuses_what_is_no_key() {
    let err = |r: Result<PublicKey, KeyError>| r.unwrap_err().to_string();
    assert_eq!(
        err(PublicKey::from_multibase("Q3sh")),
        "a multikey is base58btc, starting with z"
    );
    assert_eq!(
        err(PublicKey::from_multibase("z0OIl")),
        "bad base58 in a key"
    );
    assert_eq!(
        err(PublicKey::from_multibase("z1")),
        "not a secp256k1 or P-256 multikey"
    );
    assert_eq!(err(PublicKey::from_did_key("did:web:x")), "not a did:key");
    assert_eq!(
        err(PublicKey::from_sec1(Curve::K256, &[2; 3])),
        "not a secp256k1 key"
    );
    assert_eq!(
        err(PublicKey::from_sec1(Curve::P256, &[2; 3])),
        "not a P-256 key"
    );
    assert_eq!(
        Keypair::from_bytes(Curve::P256, &[0; 32])
            .unwrap_err()
            .to_string(),
        "not a secret key on its curve"
    );
    assert!(Keypair::from_bytes(Curve::K256, &[0; 3]).is_err());
}
