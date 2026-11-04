//! JOSE as atproto OAuth uses it: public keys as JWKs, their RFC 7638
//! thumbprints (a DPoP key's `jkt`), and compact JWTs signed with ES256 or
//! ES256K, the two curves atproto keys are on. What jose, which the
//! reference OAuth provider uses, makes of the same keys is in
//! `tests/reference/jose.json`.

use crate::keys::{Curve, Keypair, PublicKey};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoseError(pub String);

impl fmt::Display for JoseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for JoseError {}

fn err<T>(what: impl Into<String>) -> Result<T, JoseError> {
    Err(JoseError(what.into()))
}

/// `bytes`, base64url without padding.
pub fn b64(bytes: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The JSON object `part` holds, base64url: a JWT's header or claims.
pub fn json_of(part: &str) -> Option<Value> {
    let bytes = URL_SAFE_NO_PAD.decode(part).ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
}

/// SHA-256 of `text`, base64url: a PKCE challenge, a DPoP proof's `ath`.
pub fn sha256(text: &str) -> String {
    b64(Sha256::digest(text.as_bytes()))
}

/// `bytes` random bytes, base64url: a token, a nonce, a `jti`.
pub fn random(bytes: usize) -> String {
    let mut buf = vec![0; bytes];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut buf);
    b64(buf)
}

fn crv(curve: Curve) -> &'static str {
    match curve {
        Curve::K256 => "secp256k1",
        Curve::P256 => "P-256",
    }
}

/// `key` as a public JWK.
pub fn jwk(key: &PublicKey) -> Value {
    let point = match key {
        PublicKey::K256(k) => k.to_encoded_point(false).as_bytes().to_vec(),
        PublicKey::P256(k) => k.to_encoded_point(false).as_bytes().to_vec(),
    };
    json!({
        "kty": "EC",
        "crv": crv(key.curve()),
        "x": b64(&point[1..33]),
        "y": b64(&point[33..]),
    })
}

/// The public key a JWK holds: an EC key on P-256 or secp256k1, never one
/// with its private part.
pub fn from_jwk(jwk: &Value) -> Result<PublicKey, JoseError> {
    if jwk.get("d").is_some() {
        return err("a JWK with its private key");
    }
    if jwk["kty"] != "EC" {
        return err("a JWK that is not an EC key");
    }
    let curve = match jwk["crv"].as_str() {
        Some("P-256") => Curve::P256,
        Some("secp256k1") => Curve::K256,
        other => return err(format!("a JWK on the curve {other:?}")),
    };
    let coordinate = |name: &str| {
        let text = jwk[name].as_str().unwrap_or_default();
        URL_SAFE_NO_PAD
            .decode(text)
            .ok()
            .filter(|c| c.len() == 32)
            .ok_or_else(|| JoseError(format!("a JWK whose {name} is not 32 bytes")))
    };
    let point = [&[4u8][..], &coordinate("x")?, &coordinate("y")?].concat();
    PublicKey::from_sec1(curve, &point).map_err(|e| JoseError(e.0.into()))
}

/// A JWK's RFC 7638 thumbprint, SHA-256: its required members, in order,
/// as JSON with no spaces.
pub fn thumbprint(key: &PublicKey) -> String {
    let jwk = jwk(key);
    let canonical = format!(
        r#"{{"crv":"{}","kty":"EC","x":"{}","y":"{}"}}"#,
        jwk["crv"].as_str().unwrap_or_default(),
        jwk["x"].as_str().unwrap_or_default(),
        jwk["y"].as_str().unwrap_or_default(),
    );
    b64(Sha256::digest(canonical.as_bytes()))
}

/// `claims` signed with `key` as a compact JWT, under `header` with the
/// key's `alg` added.
pub fn sign(key: &Keypair, header: &Value, claims: &Value) -> String {
    let mut header = header.clone();
    header["alg"] = key.curve().alg().into();
    let body = format!("{}.{}", b64(header.to_string()), b64(claims.to_string()));
    format!("{body}.{}", b64(key.sign(body.as_bytes())))
}

/// A compact JWT's header and claims, unchecked: to find whose key to
/// check it with.
pub fn parts(token: &str) -> Result<(Value, Value), JoseError> {
    let pieces: Vec<&str> = token.split('.').collect();
    let [header, claims, _] = pieces[..] else {
        return err("not a compact JWT");
    };
    let decode = |part: &str, what: &str| {
        json_of(part).ok_or_else(|| JoseError(format!("bad JSON in a JWT {what}")))
    };
    Ok((decode(header, "header")?, decode(claims, "payload")?))
}

/// A JWT's header and claims, once it is signed by `key` with the key's
/// `alg`. Its claims' times are the caller's to check. Either `s` is
/// taken, as jose signs with either.
pub fn verify(token: &str, key: &PublicKey) -> Result<(Value, Value), JoseError> {
    let (header, claims) = parts(token)?;
    let alg = key.curve().alg();
    if header["alg"] != alg {
        return err(format!(
            "a JWT signed with {} where {alg} was expected",
            header["alg"]
        ));
    }
    let (signed, sig) = token.rsplit_once('.').unwrap_or_default();
    let sig = URL_SAFE_NO_PAD.decode(sig).unwrap_or_default();
    if !key.verify_either_s(signed.as_bytes(), &sig) {
        return err("bad signature");
    }
    Ok((header, claims))
}
