//! What the OAuth server keeps: each pushed authorization request, until
//! its code is exchanged or it lapses, and each grant it issued tokens
//! for. A token is kept as its SHA-256, never as itself.

use crate::store::Store;
use anyhow::Result;
use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;

/// A pushed authorization request, and once a member approves it, the
/// group it is for, who approved it, what it grants, and its code.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: String,
    pub client_id: String,
    /// How the client authenticated when it pushed the request.
    pub client_auth: Value,
    /// The parameters it pushed, as the host took them.
    pub parameters: Value,
    pub expires_at: i64,
    pub group: Option<String>,
    pub member: Option<String>,
    pub scope: Option<String>,
}

/// A grant a client holds: the group it acts as, the member who approved
/// it, and what it may do. Its access and refresh tokens change on each
/// refresh; the grant stays.
#[derive(Debug, Clone, PartialEq)]
pub struct Grant {
    pub id: String,
    pub group: String,
    pub member: String,
    pub client_id: String,
    pub client_auth: Value,
    pub scope: String,
    /// The thumbprint of the DPoP key its tokens are bound to.
    pub dpop_jkt: String,
    pub access_expires: i64,
    /// When its refresh token lapses.
    pub expires_at: i64,
    pub created_at: i64,
}

const PUSH: &str = "INSERT INTO oauth_request (id, client_id, client_auth, parameters, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)";
const FORGET_LAPSED: &str = "DELETE FROM oauth_request WHERE expires_at <= ?1";
const REQUEST: &str = "SELECT id, client_id, client_auth, parameters, expires_at, grp, member, scope FROM oauth_request WHERE id = ?1 AND code IS NULL AND expires_at > ?2";
const APPROVE: &str = "UPDATE oauth_request SET grp = ?2, member = ?3, scope = ?4, code = ?5, expires_at = ?6 WHERE id = ?1 AND code IS NULL AND expires_at > ?7";
const DROP: &str = "DELETE FROM oauth_request WHERE id = ?1 AND code IS NULL AND expires_at > ?2";
const REDEEM: &str = "DELETE FROM oauth_request WHERE code = ?1 RETURNING id, client_id, client_auth, parameters, expires_at, grp, member, scope";
const ISSUE: &str = "INSERT INTO oauth_token (id, grp, member, client_id, client_auth, scope, dpop_jkt, access_hash, access_expires, refresh_hash, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";
const GRANT_COLUMNS: &str = "id, grp, member, client_id, client_auth, scope, dpop_jkt, access_expires, expires_at, created_at";
const ROTATE: &str = "UPDATE oauth_token SET access_hash = ?2, access_expires = ?3, refresh_hash = ?4, expires_at = ?5 WHERE refresh_hash = ?1 AND expires_at > ?6";
const REVOKE: &str = "DELETE FROM oauth_token WHERE access_hash = ?1 OR refresh_hash = ?1";
const REVOKE_MEMBER: &str = "DELETE FROM oauth_token WHERE grp = ?1 AND member = ?2";

fn json(text: String) -> Value {
    serde_json::from_str(&text).expect("JSON the host wrote")
}

fn request(row: &Row) -> rusqlite::Result<Request> {
    Ok(Request {
        id: row.get(0)?,
        client_id: row.get(1)?,
        client_auth: json(row.get(2)?),
        parameters: json(row.get(3)?),
        expires_at: row.get(4)?,
        group: row.get(5)?,
        member: row.get(6)?,
        scope: row.get(7)?,
    })
}

fn grant(row: &Row) -> rusqlite::Result<Grant> {
    Ok(Grant {
        id: row.get(0)?,
        group: row.get(1)?,
        member: row.get(2)?,
        client_id: row.get(3)?,
        client_auth: json(row.get(4)?),
        scope: row.get(5)?,
        dpop_jkt: row.get(6)?,
        access_expires: row.get(7)?,
        expires_at: row.get(8)?,
        created_at: row.get(9)?,
    })
}

/// A grant's tokens, as their hashes, and when each lapses.
pub struct Tokens<'a> {
    pub access_hash: &'a str,
    pub access_expires: i64,
    pub refresh_hash: &'a str,
    pub expires_at: i64,
}

impl Store {
    /// Keeps a pushed request; those lapsed by `now` are forgotten.
    pub fn push_request(&self, request: &Request, now: i64) -> Result<()> {
        let conn = self.conn();
        let forgot = conn.execute(FORGET_LAPSED, [now]);
        forgot?;
        conn.execute(
            PUSH,
            params![
                request.id,
                request.client_id,
                request.client_auth.to_string(),
                request.parameters.to_string(),
                request.expires_at,
            ],
        )?;
        Ok(())
    }

    /// The request `id`, while it waits on a member.
    pub fn pending_request(&self, id: &str, now: i64) -> Result<Option<Request>> {
        let conn = self.conn();
        let found = conn.query_row(REQUEST, params![id, now], request);
        Ok(found.optional()?)
    }

    /// Binds the waiting request `id` to the group, the member who
    /// approved it and what it grants, with its `code` until `expires_at`.
    /// `false` if it waits no longer.
    pub fn approve_request(
        &self,
        id: &str,
        approved: &Request,
        code: &str,
        now: i64,
    ) -> Result<bool> {
        let conn = self.conn();
        let changed = conn.execute(
            APPROVE,
            params![
                id,
                approved.group,
                approved.member,
                approved.scope,
                code,
                approved.expires_at,
                now
            ],
        )?;
        Ok(changed > 0)
    }

    /// Forgets the waiting request `id`. `false` if it waits no longer.
    pub fn drop_request(&self, id: &str, now: i64) -> Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(DROP, params![id, now])? > 0)
    }

    /// The approved request whose code is `code`, forgotten as it is
    /// read: a code is good once.
    pub fn redeem_code(&self, code: &str) -> Result<Option<Request>> {
        let conn = self.conn();
        let found = conn.query_row(REDEEM, [code], request);
        Ok(found.optional()?)
    }

    /// Keeps a new grant with its first tokens.
    pub fn issue(&self, grant: &Grant, tokens: &Tokens) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            ISSUE,
            params![
                grant.id,
                grant.group,
                grant.member,
                grant.client_id,
                grant.client_auth.to_string(),
                grant.scope,
                grant.dpop_jkt,
                tokens.access_hash,
                tokens.access_expires,
                tokens.refresh_hash,
                tokens.expires_at,
                grant.created_at,
            ],
        )?;
        Ok(())
    }

    /// The grant whose refresh token hashes to `hash`, while it lasts.
    pub fn grant_to_refresh(&self, hash: &str, now: i64) -> Result<Option<Grant>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {GRANT_COLUMNS} FROM oauth_token WHERE refresh_hash = ?1 AND expires_at > ?2"
        );
        let found = conn.query_row(&sql, params![hash, now], grant);
        Ok(found.optional()?)
    }

    /// The grant whose access token hashes to `hash`, while that lasts.
    pub fn grant_by_access(&self, hash: &str, now: i64) -> Result<Option<Grant>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {GRANT_COLUMNS} FROM oauth_token WHERE access_hash = ?1 AND access_expires > ?2"
        );
        let found = conn.query_row(&sql, params![hash, now], grant);
        Ok(found.optional()?)
    }

    /// Gives the grant whose refresh token hashes to `old` new tokens: the
    /// old ones stop working. `false` if it was refreshed or revoked first.
    pub fn rotate(&self, old: &str, tokens: &Tokens, now: i64) -> Result<bool> {
        let conn = self.conn();
        let changed = conn.execute(
            ROTATE,
            params![
                old,
                tokens.access_hash,
                tokens.access_expires,
                tokens.refresh_hash,
                tokens.expires_at,
                now
            ],
        )?;
        Ok(changed > 0)
    }

    /// Ends the grant either of whose tokens hashes to `hash`.
    pub fn revoke_token(&self, hash: &str) -> Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(REVOKE, [hash])? > 0)
    }

    /// Ends every grant `member` approved for `group`: theirs to act with
    /// no more once they are out of it.
    pub fn revoke_member_grants(&self, group: &str, member: &str) -> Result<usize> {
        let conn = self.conn();
        Ok(conn.execute(REVOKE_MEMBER, [group, member])?)
    }
}
