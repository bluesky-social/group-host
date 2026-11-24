//! App passwords: the way into a group from an app that signs in with a
//! handle and a password (`com.atproto.server.createSession`), as the
//! Bluesky app does, rather than with OAuth.
//!
//! A group has no password, and an app password does not give it one. A
//! member who may do anything as the group (every repo collection) makes
//! one in the console, named for where it is used, and it stays theirs: it
//! stops working the moment they could no longer make it, and its sessions
//! are listed as theirs among the group's grants. The host keeps only its
//! hash: it is 80 random bits, so a fast hash is as good as a slow one.
//!
//! A session from one is a grant like any other, its tokens bearer ones.
//! Its scope is `atproto transition:generic` (and `transition:chat.bsky`
//! for a privileged one): the group's repo and blobs, never a space.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::group_api::{group_of, signed_in};
use crate::limits::{Client, SESSIONS, key};
use crate::oauth_dpop::hmac;
use crate::oauth_resource::{Acting, Refusal};
use crate::oauth_store::{AppPassword, Grant, Tokens, app_password_client};
use crate::rbac::{NS, repo_collections_for};
use crate::scopes::Permissions;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use atproto::jose;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::{Extension, Json};
use serde_json::{Value, json};
use std::sync::Arc;

/// The most app passwords a group holds at once, as a PDS caps an
/// account's.
pub const APP_PASSWORDS_AT_MOST: usize = 25;
/// How long a session's access token lasts, and the session unrefreshed.
const ACCESS_TTL: i64 = 2 * 60 * 60_000;
const SESSION_TTL: i64 = 90 * 86_400_000;
/// What a session is, as `listGroupTokens` names it.
pub const SOURCE: &str = "appPassword";

type Answer = Result<Json<Value>, XrpcError>;

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

fn refused(error: &'static str, message: &str) -> XrpcError {
    XrpcError::new(StatusCode::UNAUTHORIZED, error, message)
}

/// `xxxx-xxxx-xxxx-xxxx` from a-z and 2-7, as a PDS hands one out: 80
/// random bits.
fn new_password() -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut bytes = [0u8; 16];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut bytes);
    let chars: Vec<char> = bytes
        .iter()
        .map(|b| ALPHABET[(b % 32) as usize] as char)
        .collect();
    let groups: Vec<String> = chars.chunks(4).map(|c| c.iter().collect()).collect();
    groups.join("-")
}

/// What the host keeps of an app password, by which a sign-in finds it.
fn hash_of(password: &str) -> String {
    jose::sha256(&password.trim().to_lowercase())
}

/// A session's scope, for an app password privileged or not.
fn scope_of(p: &AppPassword) -> &'static str {
    match p.privileged {
        true => "atproto transition:generic transition:chat.bsky",
        false => "atproto transition:generic",
    }
}

impl Host {
    /// Whether `member` may make app passwords for `group`: they may do
    /// anything as it, every repo collection.
    pub fn may_issue(&self, group: &str, member: &str) -> Result<bool, XrpcError> {
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let granted = self.role_grants(group).map_err(XrpcError::internal)?;
        Ok(repo_collections_for(&granted, &roles).contains("*"))
    }

    /// Whether a session's app password still stands: there, and its
    /// maker still able to make it. One whose maker could not now is
    /// forgotten, with every session signed in with one of theirs.
    fn stands(&self, grant: &Grant) -> Result<bool, XrpcError> {
        let name = grant
            .client_id
            .strip_prefix("app-password:")
            .unwrap_or_default();
        let found = self.store.app_password(&grant.group, Some(name), None);
        let found = found.map_err(XrpcError::internal)?;
        if found.is_some_and(|p| p.member == grant.member)
            && self.may_issue(&grant.group, &grant.member)?
        {
            return Ok(true);
        }
        self.forget_app_passwords(&grant.group, &grant.member)?;
        Ok(false)
    }

    /// Forgets `member`'s app passwords for `group`, and every session
    /// signed in with one of them.
    fn forget_app_passwords(&self, group: &str, member: &str) -> Result<(), XrpcError> {
        let dropped = self.store.drop_members_app_passwords(group, member);
        dropped.map_err(XrpcError::internal)?;
        let ended = self.store.revoke_member_sessions(group, member);
        ended.map_err(XrpcError::internal)?;
        Ok(())
    }

    /// The group's app passwords whose makers could still make them; the
    /// rest are forgotten, so none comes back with its maker's roles.
    fn app_passwords(&self, group: &str) -> Result<Vec<AppPassword>, XrpcError> {
        let held = self.store.app_passwords(group);
        let mut kept = Vec::new();
        for p in held.map_err(XrpcError::internal)? {
            match self.may_issue(group, &p.member)? {
                true => kept.push(p),
                false => self.forget_app_passwords(group, &p.member)?,
            }
        }
        Ok(kept)
    }

    /// A JWT naming a session, as a PDS's are: an app may read when it
    /// lapses. The host finds a session by its hash, so its signature is
    /// only the host's mark.
    fn session_jwt(&self, group: &str, kind: &str, exp: i64) -> String {
        let key = hmac(&self.secrets.rotation.to_bytes(), b"group-host session key");
        let claims = json!({
            "scope": kind,
            "sub": group,
            "aud": self.config.service_did,
            "iat": now_millis() / 1000,
            "exp": exp / 1000,
            "jti": jose::random(16),
        });
        let header = json!({"alg": "HS256", "typ": "JWT"});
        let body = format!(
            "{}.{}",
            jose::b64(header.to_string()),
            jose::b64(claims.to_string())
        );
        format!("{body}.{}", jose::b64(hmac(&key, body.as_bytes())))
    }

    /// The access and refresh tokens of a session, new, and their hashes.
    fn session_tokens(&self, group: &str, now: i64) -> (String, String) {
        let access = self.session_jwt(group, "com.atproto.access", now + ACCESS_TTL);
        let refresh = self.session_jwt(group, "com.atproto.refresh", now + SESSION_TTL);
        (access, refresh)
    }

    /// What createSession and refreshSession answer, as a PDS does.
    async fn session_view(&self, group: &str, access: &str, refresh: &str) -> Answer {
        let handle = group_of(self, group)?;
        let doc = self.plc.document(group).await.ok();
        let active = self.deactivation(group)?.is_none();
        let mut out = json!({
            "accessJwt": access,
            "refreshJwt": refresh,
            "did": group,
            "handle": handle,
            // A group has no email: nothing for an app to confirm.
            "emailConfirmed": false,
            "active": active,
        });
        if let Some(doc) = doc {
            out["didDoc"] = doc;
        }
        if !active {
            out["status"] = json!("deactivated");
        }
        Ok(Json(out))
    }

    /// The session a bearer access token names, while it and its app
    /// password last.
    pub(crate) fn session(&self, token: &str, now: i64) -> Result<Acting, Refusal> {
        let found = self.store.grant_by_access(&jose::sha256(token), now);
        let grant = found
            .map_err(XrpcError::internal)?
            .filter(|g| g.source == SOURCE);
        let Some(grant) = grant else {
            // One that lapsed is a 400 saying so, as a PDS answers: what an
            // app takes as its sign to refresh.
            let any = self.store.grant_by_access(&jose::sha256(token), i64::MIN);
            let lapsed = any
                .map_err(XrpcError::internal)?
                .is_some_and(|g| g.source == SOURCE);
            if lapsed {
                let said = "the token has expired";
                return Err(XrpcError::new(StatusCode::BAD_REQUEST, "ExpiredToken", said).into());
            }
            return Err(refused("InvalidToken", "the token is invalid").into());
        };
        if !self.stands(&grant)? {
            return Err(refused("InvalidToken", "the token is invalid").into());
        }
        self.store
            .touch(&grant.id, now)
            .map_err(XrpcError::internal)?;
        let permissions = Permissions::new(&grant.scope, &grant.group);
        Ok(Acting { grant, permissions })
    }

    /// The session a request's bearer refresh token names, while it lasts.
    fn refreshing(&self, headers: &HeaderMap) -> Result<(Grant, String), XrpcError> {
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let token = authorization.and_then(|a| a.strip_prefix("Bearer "));
        let token = token.ok_or_else(|| refused("AuthMissing", "a refresh token is required"))?;
        let hash = jose::sha256(token);
        let found = self.store.grant_to_refresh(&hash, now_millis());
        let grant = found
            .map_err(XrpcError::internal)?
            .filter(|g| g.source == SOURCE);
        let grant = grant.ok_or_else(|| refused("InvalidToken", "the token is invalid"))?;
        Ok((grant, hash))
    }
}

/// Signs in to a group with one of its app passwords, as often as
/// [`crate::limits::SESSIONS`] lets one handle from one address.
pub async fn create_session(
    State(host): State<Arc<Host>>,
    client: Option<Extension<Client>>,
    Json(body): Json<Value>,
) -> Answer {
    let no = || refused("AuthenticationRequired", "Invalid identifier or password");
    let identifier = body["identifier"]
        .as_str()
        .ok_or_else(|| invalid("identifier is required"))?;
    let password = body["password"]
        .as_str()
        .ok_or_else(|| invalid("password is required"))?;
    let identifier = identifier.trim().trim_start_matches('@').to_lowercase();
    let from = client.map_or_else(String::new, |Extension(Client(ip))| key(ip));
    host.spend(&SESSIONS, &format!("{identifier} {from}"), 1)?;
    let account = host
        .store
        .account(&identifier)
        .map_err(XrpcError::internal)?;
    let group = account
        .map(|a| a.did)
        .filter(|d| group_of(&host, d).is_ok())
        .ok_or_else(no)?;
    let found = host
        .store
        .app_password(&group, None, Some(&hash_of(password)));
    let p = found.map_err(XrpcError::internal)?.ok_or_else(no)?;
    if !host.may_issue(&group, &p.member)? {
        host.forget_app_passwords(&group, &p.member)?;
        return Err(no());
    }
    let now = now_millis();
    let (access, refresh) = host.session_tokens(&group, now);
    let grant = Grant {
        id: format!("tok-{}", jose::random(16)),
        group: group.clone(),
        member: p.member.clone(),
        client_id: app_password_client(&p.name),
        client_auth: json!({"method": "none"}),
        scope: scope_of(&p).into(),
        dpop_jkt: String::new(),
        access_expires: now + ACCESS_TTL,
        expires_at: now + SESSION_TTL,
        created_at: now,
        source: SOURCE.into(),
        used_at: None,
    };
    let tokens = Tokens {
        access_hash: &jose::sha256(&access),
        access_expires: grant.access_expires,
        refresh_hash: &jose::sha256(&refresh),
        expires_at: grant.expires_at,
    };
    host.store
        .issue(&grant, &tokens)
        .map_err(XrpcError::internal)?;
    let signed_in = format!("signed in with the app password \"{}\"", p.name);
    host.record(&group, &p.member, Some(&grant.client_id), &signed_in)
        .map_err(XrpcError::internal)?;
    host.session_view(&group, &access, &refresh).await
}

/// A session's new pair of tokens, for its refresh token: the old pair
/// stops working.
pub async fn refresh_session(State(host): State<Arc<Host>>, headers: HeaderMap) -> Answer {
    let (grant, old) = host.refreshing(&headers)?;
    if !host.stands(&grant)? {
        return Err(refused("InvalidToken", "the token is invalid"));
    }
    let now = now_millis();
    let (access, refresh) = host.session_tokens(&grant.group, now);
    let tokens = Tokens {
        access_hash: &jose::sha256(&access),
        access_expires: now + ACCESS_TTL,
        refresh_hash: &jose::sha256(&refresh),
        expires_at: now + SESSION_TTL,
    };
    let rotated = host
        .store
        .rotate(&old, &tokens, now)
        .map_err(XrpcError::internal)?;
    // Refreshed at once by another request, which took it.
    let fresh = rotated
        .then_some(())
        .ok_or(refused("InvalidToken", "the token is invalid"));
    fresh?;
    host.session_view(&grant.group, &access, &refresh).await
}

/// Signs a session out.
pub async fn delete_session(State(host): State<Arc<Host>>, headers: HeaderMap) -> Answer {
    let (grant, _) = host.refreshing(&headers)?;
    host.store
        .end_grant(&grant.id)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// Makes an app password for the group, for one who may do anything as
/// it. The password is in the answer and nowhere else, ever again.
pub async fn create_app_password(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.createAppPassword")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    if !host.may_issue(group, &me)? {
        return Err(forbidden(
            "an app password is for whoever may do anything as the group",
        ));
    }
    let name = body["name"].as_str().unwrap_or_default().trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "InvalidName",
            "a name of 1 to 64 characters",
        ));
    }
    let held = host.app_passwords(group)?;
    if held.len() >= APP_PASSWORDS_AT_MOST {
        let said = format!("a group holds {APP_PASSWORDS_AT_MOST} app passwords at most");
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "TooManyAppPasswords",
            said,
        ));
    }
    let password = new_password();
    let made = AppPassword {
        name: name.into(),
        member: me.clone(),
        privileged: body["privileged"] == true,
        created_at: now_millis(),
    };
    let kept = host
        .store
        .make_app_password(group, &made, &hash_of(&password));
    if !kept.map_err(XrpcError::internal)? {
        let said = "the group has an app password by that name";
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "DuplicateName",
            said,
        ));
    }
    let action = format!("made the app password \"{name}\"");
    host.record(group, &me, None, &action)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({
        "name": made.name,
        "password": password,
        "privileged": made.privileged,
        "createdAt": iso(made.created_at),
    })))
}

/// The group's app passwords, never the passwords: every one for its
/// configurers, and their own for whoever may make them.
pub async fn list_app_passwords(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.listAppPasswords")).await?;
    let group = param(&p, "group")?;
    group_of(&host, group)?;
    let configures = host.may_do(group, &me, "group.configure")?;
    if !configures && !host.may_issue(group, &me)? {
        return Err(forbidden("missing group.configure"));
    }
    let sessions = host
        .store
        .grants_of(group, now_millis())
        .map_err(XrpcError::internal)?;
    let held = host.app_passwords(group)?;
    let held: Vec<_> = held
        .into_iter()
        .filter(|p| configures || p.member == me)
        .collect();
    // Whose each is, by their handle where it can be read.
    let actors = host
        .actor_views(held.iter().map(|p| p.member.as_str()))
        .await;
    let passwords: Vec<Value> = held
        .iter()
        .map(|p| {
            let client = app_password_client(&p.name);
            let used = sessions
                .iter()
                .filter(|s| s.client_id == client)
                .filter_map(|s| s.used_at)
                .max();
            let mut view = json!({
                "name": p.name,
                "member": p.member,
                "actor": actors[&p.member],
                "privileged": p.privileged,
                "createdAt": iso(p.created_at),
            });
            if let Some(used) = used {
                view["lastUsedAt"] = json!(iso(used));
            }
            view
        })
        .collect();
    Ok(Json(json!({ "passwords": passwords })))
}

/// Revokes one of the group's app passwords, and signs out every app
/// signed in with it: its maker's own, or anyone's for its configurers.
pub async fn revoke_app_password(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.revokeAppPassword")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    let name = body["name"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    let found = host
        .store
        .app_password(group, Some(name), None)
        .map_err(XrpcError::internal)?;
    let p = found.ok_or_else(|| {
        XrpcError::new(
            StatusCode::BAD_REQUEST,
            "NotFound",
            "no app password by that name",
        )
    })?;
    if p.member != me && !host.may_do(group, &me, "group.configure")? {
        return Err(forbidden("missing group.configure"));
    }
    host.store
        .drop_app_password(group, name)
        .map_err(XrpcError::internal)?;
    let action = format!("revoked the app password \"{name}\"");
    host.record(group, &me, None, &action)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
