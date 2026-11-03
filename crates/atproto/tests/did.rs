//! DID documents, as the PLC directory serves them
//! (tests/reference/identity.json), and the host's own.

use atproto::did::{handle, kem_key, method_key, pds, service, service_document, signing_key};
use atproto::keys::{Curve, Keypair, PublicKey};
use serde_json::{Value, json};

fn reference() -> Value {
    let path = format!(
        "{}/tests/reference/identity.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn reads_a_directory_document() {
    let r = reference();
    let doc = &r["plc"][0]["document"];
    let signing = r["plc"][0]["genesis"]["verificationMethods"]["atproto"]
        .as_str()
        .unwrap();
    assert_eq!(
        signing_key(doc).unwrap(),
        PublicKey::from_did_key(signing).unwrap()
    );
    assert_eq!(pds(doc).as_deref(), Some("https://groups.example.com"));
    assert_eq!(handle(doc).as_deref(), Some("riders.groups.example.com"));
    assert_eq!(
        kem_key(doc).unwrap_err().to_string(),
        "the document names no such key"
    );
}

#[test]
fn makes_and_reads_a_service_document() {
    let key = Keypair::generate(Curve::K256).public();
    let did = "did:web:groups.example.com";
    let doc = service_document(
        did,
        &key,
        &[(
            "#atproto_pds",
            "AtprotoPersonalDataServer",
            "https://groups.example.com/",
        )],
    );
    assert_eq!(signing_key(&doc).unwrap(), key);
    assert_eq!(pds(&doc).as_deref(), Some("https://groups.example.com"));
    assert_eq!(service(&doc, "#atproto_labeler"), None);
    assert_eq!(handle(&doc), None);
    assert_eq!(doc["verificationMethod"][0]["id"], format!("{did}#atproto"));
}

#[test]
fn refuses_a_key_it_cannot_read() {
    let doc = json!({"verificationMethod": [{"id": "#atproto", "type": "Multikey"}]});
    assert_eq!(
        signing_key(&doc).unwrap_err().to_string(),
        "a key with no publicKeyMultibase"
    );
    let kem = Keypair::generate(Curve::P256).public();
    let doc = json!({"verificationMethod": [{"id": "did:plc:x#atproto_kem", "type": "Multikey", "publicKeyMultibase": kem.to_multibase()}]});
    assert_eq!(kem_key(&doc).unwrap(), kem);
    assert!(method_key(&json!({}), "#atproto").is_err());
}
