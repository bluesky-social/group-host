//! A group's repos in spaces, as their repo host: its records in each
//! space it writes to, summed into the space alpha's set hash rather than
//! a signed tree, and each write told to the space's authority.
//!
//! Who may read them is the authority's to say, by the credentials it
//! issues; `space_api` checks those.

use crate::Host;
use crate::repo::{Checked, Write, invalid};
use crate::store::{Account, Change, SpaceHead};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::{Context, Result, bail};
use atproto::service_auth::{self, Claims};
use atproto::space::{Commit, LtHash, SpaceRef, element};
use atproto::space_auth::space_host_aud;
use atproto::{Value, did};
use serde_json::json;

/// The method a repo host tells an authority of a write with.
pub const NOTIFY_WRITE: &str = "com.atproto.space.notifyWrite";

impl Host {
    /// Makes `writes` to `did`'s repo in `space`, as one revision, and
    /// tells the space's authority. Its commit.
    ///
    /// The authority not hearing of it fails nothing: the write is made,
    /// and the authority learns of it with the next.
    pub async fn space_write(
        &self,
        did: &str,
        space: &str,
        writes: &[Write],
    ) -> Result<Commit, XrpcError> {
        let space = SpaceRef::parse(space).map_err(|e| invalid(e.to_string()))?;
        let (account, commit) = self.space_commit(did, &space, writes)?;
        if let Err(e) = self.notify_write(&account, &space, &commit).await {
            tracing::warn!(space = %space, repo = did, error = %format!("{e:#}"), "notifyWrite failed");
        }
        Ok(commit)
    }

    fn space_commit(
        &self,
        did: &str,
        space: &SpaceRef,
        writes: &[Write],
    ) -> Result<(Account, Commit), XrpcError> {
        let _one_at_a_time = self.writing.lock().expect("not poisoned");
        let account = self.store.account(did).map_err(XrpcError::internal)?;
        let account = account.ok_or_else(|| {
            XrpcError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "RepoNotFound",
                format!("no repo {did} here"),
            )
        })?;
        let uri = space.to_string();
        let head = self
            .store
            .space_head(&account.did, &uri)
            .map_err(XrpcError::internal)?;
        let mut hash = match &head {
            Some(h) => LtHash::from_state(&h.state).expect("a state the host wrote"),
            None => LtHash::default(),
        };
        let mut paths = self
            .store
            .space_paths(&account.did, &uri)
            .map_err(XrpcError::internal)?;
        let mut changes = Vec::new();
        for write in writes {
            let Checked { path, put, blobs } = self.check(&account.did, write, &paths)?;
            let (collection, rkey) = write.path();
            let prev = match &put {
                Some((cid, _)) => paths.insert(path, *cid),
                None => paths.remove(&path),
            };
            if let Some(prev) = prev {
                hash.remove(&element(collection, rkey, &prev));
            }
            if let Some((cid, _)) = &put {
                hash.add(&element(collection, rkey, cid));
            }
            changes.push(Change {
                collection: collection.into(),
                rkey: rkey.into(),
                put,
                blobs,
            });
        }
        let new = SpaceHead {
            rev: self.clock.next(),
            state: hash.state(),
        };
        let prev = head.as_ref().map(|h| h.rev.as_str());
        let moved = self
            .store
            .space_commit(&account.did, &uri, prev, &new, &changes);
        let raced = format!("{did}'s repo in {uri} moved under a write");
        let orphans = moved
            .and_then(|moved| moved.context(raced))
            .map_err(XrpcError::internal)?;
        self.remove_blobs(orphans.iter().map(|cid| (account.did.as_str(), cid)));
        let commit = Commit {
            repo: account.did.clone(),
            space: uri,
            rev: new.rev,
            data: hash.digest(),
        };
        Ok((account, commit))
    }

    /// Tells `space`'s authority, at its PDS, that `account`'s repo there
    /// is now at `commit`, asking as the account.
    async fn notify_write(
        &self,
        account: &Account,
        space: &SpaceRef,
        commit: &Commit,
    ) -> Result<()> {
        let doc = self.identity.kept(&space.authority);
        let doc = match doc {
            Some(doc) => doc,
            None => self.identity.document(&space.authority).await?,
        };
        let pds = did::pds(&doc).context("the authority's document names no PDS")?;
        let now = now_millis() as u64 / 1000;
        let claims = Claims {
            iss: account.did.clone(),
            aud: space_host_aud(&space.authority),
            exp: now + 60,
            iat: Some(now),
            lxm: Some(NOTIFY_WRITE.into()),
            jti: Some(self.clock.next()),
        };
        let token = service_auth::sign(&account.signing, &claims);
        let body = json!({
            "space": commit.space,
            "repo": commit.repo,
            "repoRev": commit.rev,
            "data": Value::Bytes(commit.data.to_vec()).to_json(),
        });
        let res = self
            .http
            .post(format!("{pds}/xrpc/{NOTIFY_WRITE}"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;
        let status = res.status();
        if !status.is_success() {
            let said = res.text().await.unwrap_or_default();
            bail!("the authority refused ({status}): {said}");
        }
        Ok(())
    }
}
