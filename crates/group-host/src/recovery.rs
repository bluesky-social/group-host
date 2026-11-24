//! Recovery keys: rotation keys on a group's `did:plc` that its stewards
//! hold and the host never sees.
//!
//! A group's identity is in the hands of the rotation keys its PLC
//! document names. Left alone, the only one is this host's, so the host
//! alone could move the group, and nobody could move it without the host.
//! A recovery key sits ahead of the host's: PLC lets a key ranked higher
//! undo, for 72 hours, an operation one ranked lower signed, so whoever
//! holds one can undo anything this host does to the identity, and take
//! the group to another host without it. That is what makes leaving
//! possible. A group with one is, when deleted, left to them: its DID is
//! theirs to bring back elsewhere (`lifecycle.rs`).
//!
//! The keys are the directory's to say; the host remembers only who added
//! which, to show.

use crate::Host;
use crate::api::{Params, param};
use crate::group_api::{group_of, signed_in};
use crate::rbac::INTERNAL;
use crate::store::Store;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::PublicKey;
use atproto::plc::{Op, Signed};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// PLC takes at most five rotation keys, one of them this host's.
pub const RECOVERY_KEYS_AT_MOST: usize = 4;

fn refused(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

impl Store {
    /// Who added each of `group`'s recovery keys, and when.
    pub fn recovery_keys(&self, group: &str) -> Result<HashMap<String, (String, i64)>> {
        let conn = self.conn();
        let mut q =
            conn.prepare("SELECT key, added_by, added_at FROM recovery_key WHERE grp = ?1")?;
        let rows = q.query_map([group], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Notes that `by` added `key` to `group`, or, with no `by`, that it is
    /// gone.
    pub fn note_recovery_key(
        &self,
        group: &str,
        key: &str,
        by: Option<&str>,
        at: i64,
    ) -> Result<()> {
        let conn = self.conn();
        match by {
            Some(by) => conn.execute(
                "INSERT OR REPLACE INTO recovery_key (grp, key, added_by, added_at) VALUES (?1, ?2, ?3, ?4)",
                params![group, key, by, at],
            )?,
            None => conn.execute("DELETE FROM recovery_key WHERE grp = ?1 AND key = ?2", [group, key])?,
        };
        Ok(())
    }
}

/// A group's rotation keys as the directory holds them now.
struct Keys {
    last: Signed,
    op: Op,
    /// This host's key.
    ours: String,
    /// Every other, in the order they are ranked.
    stewards: Vec<String>,
}

impl Keys {
    fn host_holds(&self) -> bool {
        self.op.rotation_keys.contains(&self.ours)
    }
}

impl Host {
    /// The group a body or query names, for `me`, who may configure it.
    async fn configuring(
        &self,
        headers: &HeaderMap,
        lxm: &str,
        group: Option<&str>,
    ) -> Result<(String, String), XrpcError> {
        let me = signed_in(self, headers, &format!("{INTERNAL}.{lxm}")).await?;
        let group = group.expect("its lexicon requires it");
        group_of(self, group)?;
        if !self.may_do(group, &me, "group.configure")? {
            return Err(XrpcError::new(
                StatusCode::FORBIDDEN,
                "Forbidden",
                "missing group.configure",
            ));
        }
        Ok((group.to_string(), me))
    }

    async fn rotation_keys(&self, group: &str) -> Result<Keys, XrpcError> {
        let last = self.plc.last_op(group).await.map_err(XrpcError::internal)?;
        let Signed::Update { op, .. } = &last else {
            return Err(refused("NotController", "the group's DID is ended"));
        };
        let op = op.clone();
        let ours = self.secrets.rotation.public().did_key();
        let stewards = op
            .rotation_keys
            .iter()
            .filter(|k| **k != ours)
            .cloned()
            .collect();
        Ok(Keys {
            last,
            op,
            ours,
            stewards,
        })
    }

    /// The group's rotation keys, for a change to them: refused once the
    /// host holds none, whatever the change.
    async fn keys_to_change(&self, group: &str) -> Result<Keys, XrpcError> {
        let keys = self.rotation_keys(group).await?;
        if !keys.host_holds() {
            return Err(refused(
                "NotController",
                "this host no longer holds a rotation key for the group, so it cannot change them",
            ));
        }
        Ok(keys)
    }

    /// Writes the group's rotation keys: its stewards', in order, and
    /// this host's last, signed by this host's.
    async fn set_rotation_keys(
        &self,
        group: &str,
        keys: &Keys,
        stewards: Vec<String>,
    ) -> Result<(), XrpcError> {
        let mut next = keys.op.clone();
        next.rotation_keys = stewards;
        next.rotation_keys.push(keys.ours.clone());
        let signed = next.sign(Some(keys.last.cid()), &self.secrets.rotation);
        self.plc
            .publish(group, &signed)
            .await
            .map_err(crate::plc::not_published)
    }
}

/// The group's rotation keys that matter, the highest ranked first, and
/// who added each, for one who configures it.
pub async fn list_recovery_keys(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let (group, _) = host
        .configuring(&headers, "listRecoveryKeys", Some(param(&p, "group")?))
        .await?;
    let keys = host.rotation_keys(&group).await?;
    let added = host
        .store
        .recovery_keys(&group)
        .map_err(XrpcError::internal)?;
    let listed: Vec<Value> = keys
        .op
        .rotation_keys
        .iter()
        .map(|key| {
            let holder = if *key == keys.ours { "host" } else { "steward" };
            let mut view = json!({"key": key, "holder": holder});
            if let Some((by, at)) = added.get(key) {
                view["addedBy"] = json!(by);
                view["addedAt"] = json!(iso(*at));
            }
            view
        })
        .collect();
    Ok(Json(
        json!({"keys": listed, "hostHolds": keys.host_holds(), "max": RECOVERY_KEYS_AT_MOST}),
    ))
}

/// Puts a steward's key ahead of the host's, after any earlier ones.
pub async fn add_recovery_key(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, me) = host
        .configuring(&headers, "addRecoveryKey", body["group"].as_str())
        .await?;
    let key = body["key"].as_str().unwrap_or_default();
    if PublicKey::from_did_key(key).is_err() {
        return Err(refused(
            "InvalidKey",
            "not a did:key the directory takes (secp256k1 or P-256)",
        ));
    }
    let turn = host.plc.turn().await;
    let keys = host.keys_to_change(&group).await?;
    if keys.op.rotation_keys.iter().any(|k| k == key) {
        return Err(refused(
            "AlreadyAdded",
            "that key is a rotation key already",
        ));
    }
    if keys.stewards.len() >= RECOVERY_KEYS_AT_MOST {
        return Err(refused(
            "TooManyKeys",
            format!("a group has {RECOVERY_KEYS_AT_MOST} recovery keys at most"),
        ));
    }
    let mut stewards = keys.stewards.clone();
    stewards.push(key.into());
    host.set_rotation_keys(&group, &keys, stewards).await?;
    drop(turn);
    let noted = host
        .store
        .note_recovery_key(&group, key, Some(&me), now_millis());
    noted.map_err(XrpcError::internal)?;
    host.record(&group, &me, None, "added a recovery key")
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// Takes a steward's key off the group's, the host's kept.
pub async fn remove_recovery_key(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (group, me) = host
        .configuring(&headers, "removeRecoveryKey", body["group"].as_str())
        .await?;
    let key = body["key"].as_str().unwrap_or_default();
    let turn = host.plc.turn().await;
    let keys = host.keys_to_change(&group).await?;
    if key == keys.ours {
        return Err(refused(
            "HostKey",
            "this host's own key is not a recovery key",
        ));
    }
    if !keys.stewards.iter().any(|k| k == key) {
        return Err(refused(
            "UnknownKey",
            "that key is not one of the group's recovery keys",
        ));
    }
    let stewards = keys
        .stewards
        .iter()
        .filter(|k| *k != key)
        .cloned()
        .collect();
    host.set_rotation_keys(&group, &keys, stewards).await?;
    drop(turn);
    let noted = host
        .store
        .note_recovery_key(&group, key, None, now_millis());
    noted.map_err(XrpcError::internal)?;
    host.record(&group, &me, None, "removed a recovery key")
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
