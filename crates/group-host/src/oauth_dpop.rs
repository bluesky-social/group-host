//! DPoP (RFC 9449): the proof a client sends with each request to the
//! OAuth server, binding its tokens to a key it holds. A nonce is an HMAC
//! of the time, so the host checks one without keeping it; a proof is
//! good once, its `jti` spent as a token's is.
//!
//! The proof itself is checked by `atproto::dpop`; the nonces and the
//! spent `jti`s are the host's.

use crate::Host;
use crate::oauth::OAuthError;
use atproto::{dpop, jose};
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// How long a nonce is good for: its own step, and one either side.
const NONCE_STEP: i64 = 3 * 60_000;
/// How old a proof may be, in seconds, and how far a client's clock may
/// be ahead.
const PROOF_MAX_AGE: u64 = 60;
const CLOCK_SKEW: u64 = 10;

/// A proof checked: the thumbprint of the key that signed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    pub jkt: String,
}

fn invalid(description: impl Into<String>) -> OAuthError {
    OAuthError::bad("invalid_dpop_proof", description)
}

pub(crate) fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
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
        let url = format!("{}{path}", self.config.public_url);
        let request = dpop::Request {
            method,
            url: &url,
            token,
        };
        let secs = (now / 1000) as u64;
        let checked = dpop::verify(proof, &request, secs, PROOF_MAX_AGE, CLOCK_SKEW);
        let dpop::Proof { jkt, claims, .. } = checked.map_err(|e| invalid(e.0))?;
        if !claims.nonce.is_some_and(|n| self.nonce_ok(&n, now)) {
            return Err(OAuthError::bad(
                "use_dpop_nonce",
                "the authorization server requires a nonce in the DPoP proof",
            ));
        }
        let until = (claims.iat + PROOF_MAX_AGE + CLOCK_SKEW) as i64 * 1000;
        let fresh = self
            .store
            .spend(&format!("dpop:{jkt}"), &claims.jti, until, now);
        if !fresh.map_err(OAuthError::internal)? {
            return Err(invalid("a DPoP proof used before"));
        }
        Ok(Some(Proof { jkt }))
    }
}
