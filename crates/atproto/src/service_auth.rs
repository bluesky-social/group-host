//! Service auth: the short-lived JWT one server hands another to say who
//! is asking (`iss`), of whom (`aud`), and for which method (`lxm`),
//! signed with the asker's signing key, as the reference PDS writes them.

use crate::keys::{Keypair, PublicKey};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthError(pub String);

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AuthError {}

fn err<T>(what: impl Into<String>) -> Result<T, AuthError> {
    Err(AuthError(what.into()))
}

/// What a token says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claims {
    /// Who asks: a DID, with a service's `#fragment` if a service asks.
    pub iss: String,
    /// Whom it is for: a DID, or a DID and its service's `#fragment`.
    pub aud: String,
    /// When it lapses, in seconds since 1970.
    pub exp: u64,
    pub iat: Option<u64>,
    /// The method it may call.
    pub lxm: Option<String>,
    pub jti: Option<String>,
}

impl Claims {
    fn json(&self) -> Value {
        let mut claims = json!({"iss": self.iss, "aud": self.aud, "exp": self.exp});
        for (k, v) in [
            ("iat", self.iat.map(Value::from)),
            ("lxm", self.lxm.clone().map(Value::from)),
            ("jti", self.jti.clone().map(Value::from)),
        ] {
            if let Some(v) = v {
                claims[k] = v;
            }
        }
        claims
    }

    /// The account asking: `iss` without a service's `#fragment`.
    pub fn account(&self) -> &str {
        self.iss.split('#').next().unwrap_or_default()
    }
}

/// `claims` signed with `key`, as a compact JWT.
pub fn sign(key: &Keypair, claims: &Claims) -> String {
    let header = json!({"typ": "JWT", "alg": key.curve().alg()});
    let body = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.json().to_string())
    );
    format!(
        "{body}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(body.as_bytes()))
    )
}

/// A token's header and claims, unchecked: to find whose key to check
/// it with.
pub fn parts(token: &str) -> Result<(Value, Claims), AuthError> {
    let mut parts = token.split('.');
    let mut decode = |what: &str| -> Result<Value, AuthError> {
        let part = parts.next().unwrap_or_default();
        let bytes = URL_SAFE_NO_PAD
            .decode(part)
            .or_else(|_| err(format!("bad base64 in a JWT {what}")))?;
        serde_json::from_slice(&bytes).or_else(|_| err(format!("bad JSON in a JWT {what}")))
    };
    let (header, claims) = (decode("header")?, decode("payload")?);
    let text = |k: &str| claims.get(k).and_then(Value::as_str).map(String::from);
    let (Some(iss), Some(aud), Some(exp)) = (
        text("iss"),
        text("aud"),
        claims.get("exp").and_then(Value::as_u64),
    ) else {
        return err("a JWT without iss, aud and exp");
    };
    let iat = claims.get("iat").and_then(Value::as_u64);
    Ok((
        header,
        Claims {
            iss,
            aud,
            exp,
            iat,
            lxm: text("lxm"),
            jti: text("jti"),
        },
    ))
}

/// The claims of `token`, once its signature is `key`'s, its `alg` the
/// key's, and it has not lapsed by `now` (seconds since 1970).
pub fn verify(token: &str, key: &PublicKey, now: u64) -> Result<Claims, AuthError> {
    let (header, claims) = parts(token)?;
    if header.get("alg").and_then(Value::as_str) != Some(key.curve().alg()) {
        return err(format!(
            "a JWT signed with {} where {} was expected",
            header["alg"],
            key.curve().alg()
        ));
    }
    // `parts` read a header and payload, so there is a dot to split at.
    let (signed, sig) = token.rsplit_once('.').unwrap_or((token, ""));
    let sig = URL_SAFE_NO_PAD.decode(sig).unwrap_or_default();
    if !key.verify(signed.as_bytes(), &sig) {
        return err("bad signature");
    }
    if claims.exp <= now {
        return err("an expired JWT");
    }
    Ok(claims)
}

/// Whether `claims` are for `aud` and `lxm`, answering who asks.
pub fn check<'a>(claims: &'a Claims, aud: &str, lxm: &str) -> Result<&'a str, AuthError> {
    if claims.aud != aud {
        return err(format!("a token for {}, not {aud}", claims.aud));
    }
    if claims.lxm.as_deref() != Some(lxm) {
        return err(format!("a token for {:?}, not {lxm}", claims.lxm));
    }
    Ok(claims.account())
}
