//! The resource server's half of OAuth: a request to one of the group's
//! PDS methods made with a token this host issued, checked. The token is
//! sent as `Authorization: DPoP <token>`, with a proof of the request that
//! hashes it, signed by the key the token is bound to. A refusal is 401
//! with a `WWW-Authenticate` challenge and the nonce for the next proof,
//! as RFC 9449 has a resource server answer.
//!
//! As the prototype's `pds/oauth/resource.ts`.

use crate::Host;
use crate::oauth::OAuthError;
use crate::oauth_store::Grant;
use crate::scopes::Permissions;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use atproto::jose;
use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// A request made with a token: the grant it is of, and what it may do.
pub struct Acting {
    pub grant: Grant,
    pub permissions: Permissions,
}

/// Why a request was not answered: its token refused, or the method's own
/// failure.
#[derive(Debug)]
pub enum Refusal {
    Token {
        error: &'static str,
        description: String,
        nonce: String,
    },
    Xrpc(XrpcError),
}

impl From<XrpcError> for Refusal {
    fn from(e: XrpcError) -> Refusal {
        Refusal::Xrpc(e)
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let (error, description, nonce) = match self {
            Refusal::Xrpc(e) => return e.into_response(),
            Refusal::Token {
                error,
                description,
                nonce,
            } => (error, description, nonce),
        };
        let challenge = format!(
            r#"DPoP error="{error}", error_description="{}""#,
            description.replace('"', "'")
        );
        let headers = [
            (header::WWW_AUTHENTICATE, challenge),
            (header::HeaderName::from_static("dpop-nonce"), nonce),
            (
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                "dpop-nonce, www-authenticate".into(),
            ),
        ];
        let body = json!({"error": error, "message": description});
        (StatusCode::UNAUTHORIZED, headers, Json(body)).into_response()
    }
}

impl Host {
    /// Keeps word in `group`'s log of what was done as it.
    pub fn record(
        &self,
        group: &str,
        member: &str,
        client: Option<&str>,
        action: &str,
    ) -> anyhow::Result<()> {
        let activity = crate::oauth_store::Activity {
            member: member.into(),
            client_id: client.map(String::from),
            action: action.into(),
            at: now_millis(),
        };
        self.store.record_activity(group, &activity)
    }

    pub(crate) fn refuse(&self, error: &'static str, description: impl Into<String>) -> Refusal {
        Refusal::Token {
            error,
            description: description.into(),
            nonce: self.dpop_nonce(now_millis()),
        }
    }

    /// The grant the token on a request of `method` to `path` is of, once
    /// the token is current and the request proved by its key.
    pub fn acting(&self, headers: &HeaderMap, method: &str, path: &str) -> Result<Acting, Refusal> {
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let Some((scheme, token)) = authorization.and_then(|a| a.split_once(' ')) else {
            return Err(XrpcError::new(
                StatusCode::UNAUTHORIZED,
                "AuthMissing",
                "a token is required",
            )
            .into());
        };
        if !scheme.eq_ignore_ascii_case("DPoP") {
            return Err(self.refuse("invalid_token", "this host's tokens are DPoP-bound"));
        }
        let now = now_millis();
        let found = self.store.grant_by_access(&jose::sha256(token), now);
        let grant = found.map_err(XrpcError::internal)?;
        let grant =
            grant.ok_or_else(|| self.refuse("invalid_token", "an unknown or lapsed token"))?;
        let proof = self.dpop_proof(headers, method, path, Some(token), now);
        let proof = proof.map_err(|e: OAuthError| self.refuse(e.error, e.description))?;
        let proof =
            proof.ok_or_else(|| self.refuse("invalid_dpop_proof", "DPoP proof required"))?;
        if proof.jkt != grant.dpop_jkt {
            return Err(self.refuse("invalid_token", "the DPoP key is not the token's"));
        }
        self.store
            .touch(&grant.id, now)
            .map_err(XrpcError::internal)?;
        let permissions = Permissions::new(&grant.scope, &grant.group);
        Ok(Acting { grant, permissions })
    }
}
