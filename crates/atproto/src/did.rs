//! DID documents: the services and keys an account's document names, each
//! found by the end of its id, as a document may write `#atproto` or
//! `did:…#atproto`.

use crate::keys::{KeyError, PublicKey};
use serde_json::{Value, json};

/// The endpoint of the service whose id ends in `id`, as `#atproto_pds`,
/// with no trailing slash.
pub fn service(doc: &Value, id: &str) -> Option<String> {
    doc.get("service")?
        .as_array()?
        .iter()
        .find(|s| {
            s.get("id")
                .and_then(Value::as_str)
                .is_some_and(|i| i.ends_with(id))
        })?
        .get("serviceEndpoint")?
        .as_str()
        .map(|s| s.trim_end_matches('/').to_string())
}

/// The account's PDS.
pub fn pds(doc: &Value) -> Option<String> {
    service(doc, "#atproto_pds")
}

/// The key of the verification method whose id ends in `id`.
pub fn method_key(doc: &Value, id: &str) -> Result<PublicKey, KeyError> {
    let method = doc
        .get("verificationMethod")
        .and_then(Value::as_array)
        .and_then(|methods| {
            methods.iter().find(|m| {
                m.get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|i| i.ends_with(id))
            })
        })
        .ok_or(KeyError("the document names no such key"))?;
    let kind = method
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let multibase = method.get("publicKeyMultibase").and_then(Value::as_str);
    PublicKey::from_method(
        kind,
        multibase.ok_or(KeyError("a key with no publicKeyMultibase"))?,
    )
}

/// The key the account's repo, and its service tokens, are signed with.
pub fn signing_key(doc: &Value) -> Result<PublicKey, KeyError> {
    method_key(doc, "#atproto")
}

/// The key a space's syncing service agrees a secret with (the space
/// alpha's `#atproto_kem`).
pub fn kem_key(doc: &Value) -> Result<PublicKey, KeyError> {
    method_key(doc, "#atproto_kem")
}

/// The handle the document claims, its first `at://`. Whether the handle
/// claims the DID back is for whoever resolves it.
pub fn handle(doc: &Value) -> Option<String> {
    doc.get("alsoKnownAs")?
        .as_array()?
        .iter()
        .find_map(|a| a.as_str()?.strip_prefix("at://"))
        .map(String::from)
}

/// The document of a service's own `did:web`, `did`, at `endpoint`, with
/// `key` its signing key, and its services, each an id (`#…`), a type and
/// an endpoint.
pub fn service_document(did: &str, key: &PublicKey, services: &[(&str, &str, &str)]) -> Value {
    json!({
        "@context": ["https://www.w3.org/ns/did/v1", "https://w3id.org/security/multikey/v1"],
        "id": did,
        "verificationMethod": [{
            "id": format!("{did}#atproto"),
            "type": "Multikey",
            "controller": did,
            "publicKeyMultibase": key.to_multibase(),
        }],
        "service": services
            .iter()
            .map(|(id, kind, endpoint)| json!({"id": id, "type": kind, "serviceEndpoint": endpoint}))
            .collect::<Vec<_>>(),
    })
}
