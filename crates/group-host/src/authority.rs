//! Being the authority for groups' spaces: which spaces a group here has,
//! who may read and write each, the repos written in each (the writer
//! set `listRepos` answers), and telling the services that sync a space
//! of each write.
//!
//! Who may do what is the host's own to decide, from its grants: the
//! group itself always; others as granted, or anyone reading a space open
//! to all. Groups and their roles fill the grants in.
//!
//! As a repo host, it owes the authority of each space a group writes to
//! word of the write: kept in an outbox and tried again until it lands,
//! or a day has passed, as pds.js's `outbox.js` does.

use crate::Host;
use crate::rbac::{self, NS};
use crate::repo::invalid;
use crate::space::NOTIFY_WRITE;
use crate::store::{Notice, Space};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::{Context, Result};
use atproto::service_auth::{self, Claims};
use atproto::space::SpaceRef;
use atproto::space_auth::space_host_aud;
use atproto::{Value, did, syntax};
use axum::http::StatusCode;
use serde_json::json;

/// What a user would do in a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// The method an authority tells a syncer a space is gone with.
pub const NOTIFY_SPACE_DELETED: &str = "com.atproto.space.notifySpaceDeleted";

/// How long a write's word is tried before it is given up, in seconds.
pub const DELIVER_FOR: i64 = 24 * 60 * 60;
/// The wait before the first retry, doubling each after, up to an hour.
const FIRST_RETRY: i64 = 10;
const LAST_RETRY: i64 = 60 * 60;
/// How many notices one round of delivery tries.
const ROUND: u32 = 50;

/// The wait before retry number `attempts`, counted from 1, in seconds.
pub fn retry_delay(attempts: i64) -> i64 {
    let doubled = FIRST_RETRY.saturating_mul(1 << (attempts - 1).clamp(0, 20));
    doubled.min(LAST_RETRY)
}

fn now() -> i64 {
    now_millis() / 1000
}

/// What delivering a notice came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Delivered,
    /// It may land later: the authority was not reached, or said so.
    Retry,
    /// It never will: the authority refused it.
    Failed,
}

impl Host {
    /// Service auth from `iss` to `aud` for `lxm`, for a minute.
    pub(crate) fn claims(&self, iss: &str, aud: &str, lxm: &str) -> Claims {
        let now = now() as u64;
        Claims {
            iss: iss.into(),
            aud: aud.into(),
            exp: now + 60,
            iat: Some(now),
            lxm: Some(lxm.into()),
            jti: Some(self.clock.next()),
        }
    }

    /// Makes a space of `authority`'s, a group here, of `kind` at `skey`;
    /// open to any reader with `public_read`. Its URI.
    pub fn create_space(
        &self,
        authority: &str,
        kind: &str,
        skey: &str,
        public_read: bool,
    ) -> Result<String, XrpcError> {
        let uri = format!("at://{authority}/space/{kind}/{skey}");
        SpaceRef::parse(&uri).map_err(|e| invalid(e.to_string()))?;
        let account = self.store.account(authority).map_err(XrpcError::internal)?;
        if !account.is_some_and(|a| a.did == authority) {
            return Err(invalid(format!("{authority} is no group here")));
        }
        let made = self
            .store
            .create_space(&uri, authority, public_read, now())
            .map_err(XrpcError::internal)?;
        if !made {
            return Err(XrpcError::new(
                StatusCode::BAD_REQUEST,
                "SpaceAlreadyExists",
                format!("{uri} exists"),
            ));
        }
        Ok(uri)
    }

    /// The space at `uri`, if its authority is a group here.
    pub fn hosted_space(&self, uri: &str) -> Result<Space, XrpcError> {
        let space = self.store.space(uri).map_err(XrpcError::internal)?;
        space.ok_or_else(|| {
            XrpcError::new(StatusCode::BAD_REQUEST, "SpaceNotFound", "space not found")
        })
    }

    /// Lets `did` read `space`, write it, or both: what it was let do
    /// before is replaced.
    pub fn grant(&self, space: &str, did: &str, read: bool, write: bool) -> Result<(), XrpcError> {
        self.hosted_space(space)?;
        if !syntax::is_did(did) {
            return Err(invalid(format!("{did} is no DID")));
        }
        let granted = self.store.grant(space, did, read, write);
        granted.map_err(XrpcError::internal)
    }

    /// Takes back whatever `did` was let do in `space`.
    pub fn ungrant(&self, space: &str, did: &str) -> Result<(), XrpcError> {
        self.hosted_space(space)?;
        self.store.ungrant(space, did).map_err(XrpcError::internal)
    }

    /// Whether `user` may do `access` in `space` (at `uri`): with no user,
    /// whether anyone may read it.
    pub fn may(
        &self,
        uri: &str,
        space: &Space,
        user: Option<&str>,
        access_kind: Access,
    ) -> Result<bool> {
        // Nothing reads or writes in the spaces of an authority switched off.
        if self.store.deactivation(&space.authority)?.is_some() {
            return Ok(false);
        }
        let Some(user) = user else {
            return Ok(access_kind == Access::Read && space.public_read);
        };
        // The authority never locks itself out of its own space.
        if user == space.authority {
            return Ok(true);
        }
        // A group's space with an access record: the record decides,
        // against the user's roles in the group.
        let access = self.group_record(&space.authority, uri, &format!("{NS}.access"), "self")?;
        if let Some(access) = access.as_ref().map(rbac::Access::of) {
            let roles = self.roles(&space.authority, user)?;
            let by_record = match access_kind {
                Access::Read => access.can_read(&roles, user),
                Access::Write => access.can_write_own(&roles, user),
            };
            if by_record {
                return Ok(true);
            }
        }
        let granted = self.store.granted(uri, user)?;
        Ok(match access_kind {
            Access::Read => space.public_read || granted.is_some_and(|(read, _)| read),
            Access::Write => granted.is_some_and(|(_, write)| write),
        })
    }

    /// A write to `space`, of a group here, by `repo`: kept in the writer
    /// set unless it knows a later one, and told to the space's syncers.
    pub(crate) fn record_writer(
        &self,
        space: &str,
        authority: &str,
        repo: &str,
        repo_rev: &str,
        data: &[u8],
    ) -> Result<()> {
        let space_rev = self.clock.next();
        let kept = self
            .store
            .record_writer(space, repo, repo_rev, data, &space_rev)?;
        let Some(prev) = kept else {
            return Ok(());
        };
        let mut body = json!({
            "space": space,
            "repo": repo,
            "repoRev": repo_rev,
            "data": Value::Bytes(data.to_vec()).to_json(),
            "spaceRev": space_rev,
        });
        if let Some(prev) = prev {
            body["prevSpaceRev"] = json!(prev);
        }
        let recipients = self.store.recipients(space, now())?;
        let signing = self
            .store
            .account(authority)?
            .context("the authority is here")?
            .signing;
        for (service, endpoint) in recipients {
            let claims = self.claims(authority, &service, NOTIFY_WRITE);
            let token = service_auth::sign(&signing, &claims);
            let send = self
                .http
                .post(format!("{endpoint}/xrpc/{NOTIFY_WRITE}"))
                .bearer_auth(token)
                .json(&body)
                .send();
            // Best effort, after the write: a syncer that misses one finds
            // out with the next, or on its own next read.
            tokio::spawn(async move {
                let said = send.await.and_then(|r| r.error_for_status());
                if let Err(e) = said {
                    tracing::warn!(service, error = %e, "forwarding notifyWrite failed");
                }
            });
        }
        Ok(())
    }

    /// Tells the services syncing each of `authority`'s spaces that it is
    /// gone, as the authority, before its key is: best effort, as a syncer
    /// that misses it learns so when its credential is refused.
    pub fn tell_spaces_deleted(&self, authority: &str) -> Result<()> {
        let signing = self
            .store
            .account(authority)?
            .context("the authority is here")?
            .signing;
        for space in self.store.spaces_of(authority)? {
            for (service, endpoint) in self.store.recipients(&space, now())? {
                let claims = self.claims(authority, &service, NOTIFY_SPACE_DELETED);
                let send = self
                    .http
                    .post(format!("{endpoint}/xrpc/{NOTIFY_SPACE_DELETED}"))
                    .bearer_auth(service_auth::sign(&signing, &claims))
                    .json(&json!({ "space": space }))
                    .send();
                tokio::spawn(async move {
                    let said = send.await.and_then(|r| r.error_for_status());
                    if let Err(e) = said {
                        tracing::warn!(service, error = %e, "notifySpaceDeleted failed");
                    }
                });
            }
        }
        Ok(())
    }

    /// Tries `notice` now: tells its space's authority, at its PDS, as the
    /// repo.
    pub async fn deliver(&self, notice: &Notice) -> Delivery {
        let Ok(space) = SpaceRef::parse(&notice.space) else {
            return Delivery::Failed;
        };
        let Ok(Some(account)) = self.store.account(&notice.repo) else {
            return Delivery::Failed;
        };
        let doc = match self.identity.kept(&space.authority) {
            Some(doc) => Ok(doc),
            None => self.identity.document(&space.authority).await,
        };
        // An authority not found, or naming no PDS yet, may be found later.
        let Some(pds) = doc.ok().as_ref().and_then(did::pds) else {
            return Delivery::Retry;
        };
        let aud = space_host_aud(&space.authority);
        let token = service_auth::sign(
            &account.signing,
            &self.claims(&account.did, &aud, NOTIFY_WRITE),
        );
        let body = json!({
            "space": notice.space,
            "repo": notice.repo,
            "repoRev": notice.rev,
            "data": Value::Bytes(notice.data.clone()).to_json(),
        });
        let sent = self
            .http
            .post(format!("{pds}/xrpc/{NOTIFY_WRITE}"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await;
        match sent.map(|r| r.status()) {
            Ok(s) if s.is_success() => Delivery::Delivered,
            Ok(s) if retryable(s) => Delivery::Retry,
            Ok(_) => Delivery::Failed,
            Err(_) => Delivery::Retry,
        }
    }

    /// Tries `notice`, and keeps what came of it: owed no more, or owed
    /// again after a wait.
    pub(crate) async fn try_notice(&self, notice: &Notice) -> Result<Delivery> {
        let delivery = self.deliver(notice).await;
        match delivery {
            Delivery::Retry => {
                let next = now() + retry_delay(notice.attempts + 1);
                self.store.retry(notice, next)?;
            }
            Delivery::Delivered => self.store.delivered(notice)?,
            Delivery::Failed => {
                tracing::warn!(
                    space = notice.space,
                    repo = notice.repo,
                    "the authority refused notifyWrite"
                );
                self.store.delivered(notice)?;
            }
        }
        Ok(delivery)
    }

    /// Tries the notices owed now, and gives up those a day old. How many
    /// were tried.
    pub async fn deliver_due(&self) -> Result<usize> {
        for (repo, space, rev) in self.store.lapse(now())? {
            tracing::warn!(repo, space, rev, "notifyWrite given up after a day");
        }
        let due = self.store.due(now(), ROUND)?;
        for notice in &due {
            self.try_notice(notice).await?;
        }
        Ok(due.len())
    }
}

/// Whether a refusal says the same request may land later.
fn retryable(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429) || status.is_server_error()
}
