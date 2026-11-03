//! How the space alpha says who may read a space: tokens and signed
//! requests, as `@atproto/space`'s `credential.ts` and `http-signature.ts`
//! have them.
//!
//! - A delegation token: a member's PDS vouches, for a minute and once, that
//!   its account asks a space's authority for access.
//! - A space credential: the authority grants a member access to a space for
//!   up to an hour, bound to a key of the member's (`cnf.kid`).
//! - A client attestation: an app vouches for itself.
//!
//! Each request made with a credential is signed with its key, as an HTTP
//! message signature over the credential and the audience it is for.

use crate::keys::{Keypair, PublicKey};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::fmt;

/// A refusal, with the code the alpha gives it (`BadJwt`, `JwtExpired`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for TokenError {}

fn refuse<T>(code: &'static str, message: impl Into<String>) -> Result<T, TokenError> {
    Err(TokenError {
        code,
        message: message.into(),
    })
}

/// The service a space's authority answers as, which a delegation token
/// and a client attestation name as their audience.
pub fn space_host_aud(authority: &str) -> String {
    format!("{authority}#atproto_pds")
}

/// A credential lives at most an hour.
pub const CREDENTIAL_MAX_AGE: u64 = 3600;
/// How far a clock may be off.
pub const CLOCK_SKEW: u64 = 5;

/// The kinds of token, each with its `typ` and its rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Delegation,
    Credential,
    ClientAttestation,
}

impl Kind {
    pub fn typ(self) -> &'static str {
        match self {
            Kind::Delegation => "atproto-space-delegation+jwt",
            Kind::Credential => "atproto-space-credential+jwt",
            Kind::ClientAttestation => "atproto-client-attestation+jwt",
        }
    }

    /// The `kid` its header always has, if one.
    fn kid(self) -> Option<&'static str> {
        (self != Kind::ClientAttestation).then_some("#atproto")
    }

    fn lifetime(self) -> u64 {
        if self == Kind::Credential { 600 } else { 60 }
    }

    fn needs_aud(self) -> bool {
        self != Kind::Credential
    }
}

/// What a token says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub iss: String,
    pub sub: String,
    pub aud: Option<String>,
    pub iat: u64,
    pub exp: u64,
    pub jti: String,
    /// The `did:key` a credential is bound to.
    pub cnf: Option<String>,
}

/// What a new token is to say beside its kind's rules.
#[derive(Debug, Clone, Default)]
pub struct Make<'a> {
    pub iss: &'a str,
    pub sub: &'a str,
    pub aud: Option<&'a str>,
    /// For a credential, the key it is bound to.
    pub key_id: Option<&'a str>,
    /// Seconds it lasts, if not its kind's.
    pub lifetime: Option<u64>,
    /// A client attestation's `kid`; the others' is always `#atproto`.
    pub kid: Option<&'a str>,
}

fn b64_json(v: &Value) -> String {
    URL_SAFE_NO_PAD.encode(v.to_string())
}

/// A token of `kind` saying `make`, signed with `key` at `now`.
pub fn create(kind: Kind, make: &Make, key: &Keypair, now: u64) -> Result<String, TokenError> {
    if kind.needs_aud() && make.aud.is_none() {
        return refuse("BadJwt", format!("a {kind:?} token needs an aud"));
    }
    if kind == Kind::Credential && make.key_id.is_none() {
        return refuse("BadJwt", "a credential needs a key id");
    }
    if kind.kid().is_some() && make.kid.is_some() {
        return refuse(
            "BadJwt",
            format!("a {kind:?} token's kid is always #atproto"),
        );
    }
    let lifetime = make.lifetime.unwrap_or(kind.lifetime());
    if kind == Kind::Credential && !(1..=CREDENTIAL_MAX_AGE).contains(&lifetime) {
        return refuse("BadJwt", "invalid space credential lifetime");
    }
    let mut header = json!({"alg": key.curve().alg(), "typ": kind.typ()});
    if let Some(kid) = make.kid.or(kind.kid()) {
        header["kid"] = json!(kid);
    }
    let jti: String = (0..16)
        .map(|_| {
            format!(
                "{:x}",
                rand_core::RngCore::next_u32(&mut rand_core::OsRng) % 16
            )
        })
        .collect();
    let mut payload =
        json!({"iss": make.iss, "sub": make.sub, "iat": now, "exp": now + lifetime, "jti": jti});
    if let Some(aud) = make.aud {
        payload["aud"] = json!(aud);
    }
    if let Some(key_id) = make.key_id {
        payload["cnf"] = json!({"kid": key_id});
    }
    let input = format!("{}.{}", b64_json(&header), b64_json(&payload));
    Ok(format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(input.as_bytes()))
    ))
}

/// A token's header and claims, checked for shape alone.
pub fn parse(kind: Kind, jwt: &str) -> Result<(Value, Token), TokenError> {
    let parts: Vec<&str> = jwt.split('.').collect();
    let [header, payload, _] = parts[..] else {
        return refuse("BadJwt", "malformed token: expected 3 parts");
    };
    let decode = |part: &str, what: &str| {
        let bytes = URL_SAFE_NO_PAD.decode(part).unwrap_or_default();
        serde_json::from_slice::<Value>(&bytes)
            .or_else(|_| refuse("BadJwt", format!("could not parse token {what}")))
    };
    let (header, payload) = (decode(header, "header")?, decode(payload, "payload")?);
    if header["typ"].as_str() != Some(kind.typ()) {
        return refuse(
            "BadJwtType",
            format!("wrong token type: expected {}", kind.typ()),
        );
    }
    if header["alg"].as_str().is_none_or(str::is_empty) {
        return refuse("BadJwt", "missing token alg");
    }
    if kind.kid().is_some() && header["kid"].as_str() != kind.kid() {
        return refuse("BadJwt", "wrong token kid: expected #atproto");
    }
    let text = |k: &str| {
        payload[k]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    let (Some(iss), Some(sub)) = (text("iss"), text("sub")) else {
        return refuse("BadJwtIss", "missing token iss or sub");
    };
    let (Some(exp), iat) = (payload["exp"].as_u64(), payload["iat"].as_u64()) else {
        return refuse("BadJwt", "missing token exp");
    };
    let aud = text("aud");
    if kind.needs_aud() && aud.is_none() {
        return refuse("BadJwtAudience", "missing token aud");
    }
    let cnf = payload["cnf"]["kid"]
        .as_str()
        .filter(|k| crate::syntax::is_did(k))
        .map(String::from);
    if kind == Kind::Credential && cnf.is_none() {
        return refuse("BadJwtCnf", "missing token cnf.kid");
    }
    let Some(jti) = text("jti") else {
        return refuse("BadJwt", format!("a {kind:?} token needs a jti"));
    };
    let iat = iat.unwrap_or(0);
    if kind == Kind::Credential && (exp <= iat || exp - iat > CREDENTIAL_MAX_AGE) {
        return refuse("BadJwt", "invalid space credential lifetime");
    }
    if kind == Kind::ClientAttestation && iss != sub {
        return refuse(
            "BadJwtIss",
            "a client attestation's iss and sub are both its client id",
        );
    }
    Ok((
        header,
        Token {
            iss,
            sub,
            aud,
            iat,
            exp,
            jti,
            cnf,
        },
    ))
}

/// What a token is checked against beside its signature and time.
#[derive(Debug, Clone, Default)]
pub struct Expect<'a> {
    pub aud: Option<&'a str>,
    pub sub: Option<&'a str>,
}

/// The claims of `jwt`, a token of `kind` signed by `key` (its issuer's),
/// alive at `now` and as `expect` asks. A caller whose cached key fails
/// asks again with the issuer's key read afresh.
pub fn verify(
    kind: Kind,
    jwt: &str,
    key: &PublicKey,
    expect: &Expect,
    now: u64,
) -> Result<Token, TokenError> {
    let (header, token) = parse(kind, jwt)?;
    if kind == Kind::Credential && token.iat > now + CLOCK_SKEW {
        return refuse("BadJwt", "space credential issued in the future");
    }
    if now.saturating_sub(CLOCK_SKEW) >= token.exp {
        return refuse("JwtExpired", "token expired");
    }
    if expect.aud.is_some() && token.aud.as_deref() != expect.aud {
        return refuse(
            "BadJwtAudience",
            "token audience does not match this service",
        );
    }
    if expect.sub.is_some_and(|s| s != token.sub) {
        return refuse(
            "BadJwtSub",
            "token subject does not match the requested space",
        );
    }
    let (input, sig) = jwt.rsplit_once('.').expect("parsed as three parts");
    let sig = URL_SAFE_NO_PAD.decode(sig).unwrap_or_default();
    let signed =
        header["alg"].as_str() == Some(key.curve().alg()) && key.verify(input.as_bytes(), &sig);
    if !signed {
        return refuse("BadJwtSignature", "invalid token signature");
    }
    Ok(token)
}

/// What a request made with a credential is signed as.
const LABEL: &str = "atproto-space-credential";
const COVERED: &str = "(\"authorization\" \"atproto-space-audience\")";

fn signature_base(authorization: &str, audience: &str, input: &str) -> String {
    format!(
        "\"authorization\": {}\n\"atproto-space-audience\": {}\n\"@signature-params\": {input}",
        authorization.trim(),
        audience.trim()
    )
}

/// The headers of a request made with `authorization` (its credential, as
/// `Atproto-Space-Credential <jwt>`) for `audience`, signed with `key`, the
/// P-256 key the credential is bound to.
pub fn sign_request(
    key: &Keypair,
    authorization: &str,
    audience: &str,
) -> Result<[(&'static str, String); 4], TokenError> {
    if key.curve() != crate::keys::Curve::P256 {
        return refuse("BadSignature", "signature key must be a P-256 did:key");
    }
    let input = format!("{COVERED};tag=\"{LABEL}\"");
    let signature = key.sign(signature_base(authorization, audience, &input).as_bytes());
    Ok([
        ("authorization", authorization.to_string()),
        ("atproto-space-audience", audience.to_string()),
        ("signature-input", format!("{LABEL}={input}")),
        (
            "signature",
            format!("{LABEL}=:{}:", STANDARD.encode(signature)),
        ),
    ])
}

/// The value of the dictionary member `label` in a structured-field
/// header, as written: what follows `label=` up to the next member.
fn member<'a>(header: &'a str, label: &str) -> Option<&'a str> {
    let (mut depth, mut quoted, mut start) = (0, false, 0);
    let bytes = header.as_bytes();
    for i in 0..=bytes.len() {
        let c = bytes.get(i).copied();
        match c {
            Some(b'"') => quoted = !quoted,
            Some(b'(') if !quoted => depth += 1,
            Some(b')') if !quoted => depth -= 1,
            Some(b',') | None if !quoted && depth == 0 => {
                let item = header[start..i].trim();
                if let Some(value) = item.strip_prefix(label).and_then(|r| r.strip_prefix('=')) {
                    return Some(value);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether a request's headers (`header` reads each by its lower-case name)
/// carry a valid signature by `key_id`, the `did:key` its credential is
/// bound to, over that credential and its audience.
pub fn verify_request<'a>(
    header: impl Fn(&str) -> Option<&'a str>,
    key_id: &str,
) -> Result<(), TokenError> {
    let bad = |what: &str| refuse("BadSignature", what.to_string());
    let (Some(authorization), Some(audience)) =
        (header("authorization"), header("atproto-space-audience"))
    else {
        return bad("missing the authorization or atproto-space-audience header");
    };
    let input = header("signature-input").and_then(|h| member(h, LABEL));
    let signature = header("signature").and_then(|h| member(h, LABEL));
    let (Some(input), Some(signature)) = (input, signature) else {
        return bad("missing or malformed signature headers");
    };
    let Some(params) = input.strip_prefix(COVERED) else {
        return bad(
            "signature must cover exactly (\"authorization\" \"atproto-space-audience\"), in order",
        );
    };
    let params: Vec<(&str, &str)> = params
        .split(';')
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.split_once('=')
                .map_or((p, ""), |(k, v)| (k, v.trim_matches('"')))
        })
        .collect();
    let param = |k: &str| params.iter().find(|(key, _)| *key == k).map(|(_, v)| *v);
    if param("tag") != Some(LABEL) {
        return bad("signature tag must be atproto-space-credential");
    }
    if param("alg").is_some_and(|a| a != "ecdsa-p256-sha256") {
        return bad("signature algorithm must be ecdsa-p256-sha256");
    }
    if param("keyid").is_some_and(|k| k != key_id) {
        return bad("signature keyid does not match the credential key");
    }
    let Ok(key @ PublicKey::P256(_)) = PublicKey::from_did_key(key_id) else {
        return bad("signature key must be a P-256 did:key");
    };
    let sig = signature
        .strip_prefix(':')
        .and_then(|s| s.strip_suffix(':'));
    let sig = sig
        .and_then(|s| STANDARD.decode(s).ok())
        .unwrap_or_default();
    let base = signature_base(authorization, audience, input);
    if sig.len() != 64 || !key.verify_either_s(base.as_bytes(), &sig) {
        return bad("invalid HTTP message signature");
    }
    Ok(())
}
