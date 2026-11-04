//! DPoP (RFC 9449): the proof a client sends with each request to the
//! OAuth server, binding its tokens to a key it holds. A nonce is an HMAC
//! of the time, so the host checks one without keeping it; a proof is
//! good once, its `jti` spent as a token's is.
//!
//! As the prototype's `pds/oauth/dpop.ts`, for the keys atproto uses:
//! ES256 and ES256K.

use crate::Host;
use crate::oauth::OAuthError;
use atproto::jose;
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// How long a nonce is good for: its own step, and one either side.
const NONCE_STEP: i64 = 3 * 60_000;
/// How old a proof may be, in seconds, and how far a client's clock may
/// be ahead.
const PROOF_MAX_AGE: i64 = 60;
const CLOCK_SKEW: i64 = 10;

/// A proof checked: the thumbprint of the key that signed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    pub jkt: String,
}

fn invalid(description: impl Into<String>) -> OAuthError {
    OAuthError::bad("invalid_dpop_proof", description)
}

fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// `url` without its query or fragment, as `htu` names a request.
fn bare(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or_default()
}

impl Host {
    fn nonce_at(&self, step: i64) -> String {
        // A key of its own for nonces, derived from the host's secret: the
        // rotation key itself signs nothing but PLC operations.
        let key = hmac(
            &self.secrets.rotation.to_bytes(),
            b"group-host dpop nonce key",
        );
        jose::b64(hmac(&key, format!("dpop-nonce/{step}").as_bytes()))
    }

    /// The nonce a client is to put in its next proof.
    pub fn dpop_nonce(&self, now: i64) -> String {
        self.nonce_at(now.div_euclid(NONCE_STEP))
    }

    fn nonce_ok(&self, nonce: &str, now: i64) -> bool {
        let step = now.div_euclid(NONCE_STEP);
        (step - 1..=step + 1).any(|s| self.nonce_at(s) == nonce)
    }

    /// The DPoP proof on a request of `method` to `path` here, checked:
    /// none if it carries none. `token` is the access token a request to
    /// a resource sends, which the proof must hash.
    pub fn dpop_proof(
        &self,
        headers: &HeaderMap,
        method: &str,
        path: &str,
        token: Option<&str>,
        now: i64,
    ) -> Result<Option<Proof>, OAuthError> {
        let mut sent = headers.get_all("dpop").iter();
        let Some(proof) = sent.next() else {
            return Ok(None);
        };
        if sent.next().is_some() {
            return Err(invalid("one DPoP proof, not several"));
        }
        let proof = proof.to_str().unwrap_or_default();
        let (header, _) = jose::parts(proof).map_err(|e| invalid(e.0))?;
        if header["typ"] != "dpop+jwt" {
            return Err(invalid("a DPoP proof is typed dpop+jwt"));
        }
        let key = jose::from_jwk(&header["jwk"]).map_err(|e| invalid(e.0))?;
        let (_, claims) = jose::verify(proof, &key).map_err(|e| invalid(e.0))?;
        let jti = claims["jti"].as_str().filter(|j| !j.is_empty());
        let jti = jti.ok_or_else(|| invalid("a DPoP proof without a jti"))?;
        if claims["htm"] != method {
            return Err(invalid("the DPoP proof's htm is not this request's"));
        }
        let htu = claims["htu"].as_str().map(bare);
        if htu != Some(&format!("{}{path}", self.config.public_url)) {
            return Err(invalid("the DPoP proof's htu is not this request's"));
        }
        let secs = now / 1000;
        let iat = claims["iat"].as_i64().unwrap_or_default();
        if iat < secs - PROOF_MAX_AGE - CLOCK_SKEW || iat > secs + CLOCK_SKEW {
            return Err(invalid("a DPoP proof too old, or from the future"));
        }
        let nonce = claims["nonce"].as_str();
        if !nonce.is_some_and(|n| self.nonce_ok(n, now)) {
            return Err(OAuthError::bad(
                "use_dpop_nonce",
                "the authorization server requires a nonce in the DPoP proof",
            ));
        }
        if token.is_some_and(|t| claims["ath"] != jose::sha256(t)) {
            return Err(invalid("the DPoP proof's ath is not this token's"));
        }
        let jkt = jose::thumbprint(&key);
        let until = (iat + PROOF_MAX_AGE + CLOCK_SKEW) * 1000;
        let fresh = self.store.spend(&format!("dpop:{jkt}"), jti, until, now);
        if !fresh.map_err(OAuthError::internal)? {
            return Err(invalid("a DPoP proof used before"));
        }
        Ok(Some(Proof { jkt }))
    }
}
