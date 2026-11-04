//! A group's repos in spaces, as their repo host: its records in each
//! space it writes to, summed into the space alpha's set hash rather than
//! a signed tree, and each write told to the space's authority.
//!
//! Who may read them is the authority's to say, by the credentials it
//! issues; `space_api` checks those.

use crate::Host;
use crate::authority::{Access, DELIVER_FOR};
use crate::repo::{Asked, Checked, Write, invalid, resolve};
use crate::store::{Change, Notice, SpaceHead};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::{Context, Result, bail};
use atproto::space::{Commit, LtHash, SpaceRef, element};

/// The method a repo host tells an authority of a write with.
pub const NOTIFY_WRITE: &str = "com.atproto.space.notifyWrite";

impl Host {
    /// Makes `writes` to `did`'s repo in `space`, as one revision, and
    /// tells the space's authority. Its commit.
    ///
    /// The authority not hearing of it fails nothing: the write is made.
    /// An authority here keeps it at once; one elsewhere is owed word of
    /// it, tried now and again until it lands or a day has passed.
    pub async fn space_write(
        &self,
        did: &str,
        space: &str,
        writes: &[Write],
    ) -> Result<Commit, XrpcError> {
        let made = self
            .space_write_asked(did, space, writes, &Asked::default())
            .await?;
        Ok(made.expect("writes that are not puts make a revision"))
    }

    /// Makes `writes` to `did`'s repo in `space` as `asked` has them, and
    /// tells the space's authority: none if, as puts, they change nothing.
    pub async fn space_write_asked(
        &self,
        did: &str,
        space: &str,
        writes: &[Write],
        asked: &Asked<'_>,
    ) -> Result<Option<Commit>, XrpcError> {
        let space = SpaceRef::parse(space).map_err(|e| invalid(e.to_string()))?;
        let Some(commit) = self.space_commit(did, &space, writes, asked)? else {
            return Ok(None);
        };
        if let Err(e) = self.tell_authority(&space, &commit).await {
            tracing::warn!(space = %space, repo = did, error = %format!("{e:#}"), "telling the authority failed");
        }
        Ok(Some(commit))
    }

    async fn tell_authority(&self, space: &SpaceRef, commit: &Commit) -> Result<()> {
        let uri = space.to_string();
        if let Some(hosted) = self.store.space(&uri)? {
            if !self.may(&uri, &hosted, Some(&commit.repo), Access::Write)? {
                bail!("{} may not write in {uri}", commit.repo);
            }
            let data = commit.data;
            return self.record_writer(&uri, &hosted.authority, &commit.repo, &commit.rev, &data);
        }
        let notice = Notice {
            repo: commit.repo.clone(),
            space: uri,
            rev: commit.rev.clone(),
            data: commit.data.to_vec(),
            attempts: 0,
        };
        let now = now_millis() / 1000;
        self.store.owe(&notice, now, now + DELIVER_FOR)?;
        self.try_notice(&notice).await?;
        Ok(())
    }

    fn space_commit(
        &self,
        did: &str,
        space: &SpaceRef,
        writes: &[Write],
        asked: &Asked,
    ) -> Result<Option<Commit>, XrpcError> {
        let _one_at_a_time = self.writing.lock().expect("not poisoned");
        let account = self.store.account(did).map_err(XrpcError::internal)?;
        let account = account.ok_or_else(|| {
            XrpcError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "RepoNotFound",
                format!("no repo {did} here"),
            )
        })?;
        self.active(&account.did)?;
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
        let writes = resolve(writes, &paths, asked)?;
        if asked.put && writes.is_empty() {
            return Ok(None);
        }
        let mut changes = Vec::new();
        for write in &writes {
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
        Ok(Some(commit))
    }
}
