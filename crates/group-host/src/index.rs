//! The host's index of its groups' spaces: what each writer has written
//! in each, read from wherever their repo is, so the host can answer for
//! the space (`listSpaceRecords`) to whoever its access record lets read.
//!
//! The host is each space's authority, so it knows its writer set: each
//! writer's revision and digest as their host last said. When a writer's
//! digest is not the one the index came to, the index reads their repo:
//! from the store, for a writer here; otherwise from their PDS, as any
//! syncer reads a space, with a credential the group issues itself and a
//! secret agreed against the writer's `#atproto_kem` key for each request.
//! It reads on from its oplog when it has read the repo before, moving
//! the set hash by each write, and checks it against the commit their
//! host vouches for; or reads it whole, as a STAR archive, and checks it
//! the same way. A record that is not a DRISL map is left out.

use crate::Host;
use crate::authority::Access;
use crate::store::SpaceHead;
use crate::time::now_millis;
use anyhow::{Context, Result, bail, ensure};
use atproto::space::{
    COMMIT_MAC, Commit, LtHash, accept_signature, check_commit_mac, element, encapsulate,
    read_repo_star,
};
use atproto::space_auth::{self, Kind, Make, sign_request};
use atproto::{Cid, Curve, Keypair, Value, did, drisl};
use serde_json::Value as Json;
use std::collections::{BTreeMap, HashSet};

/// The most bytes a repo, or a page of its oplog, is read for.
pub const REPO_AT_MOST: usize = 64 * 1024 * 1024;

/// How many writes a page of an oplog is asked for.
const PAGE: &str = "500";

/// What the index learned of a repo: by path, each record's CID and
/// DRISL, or none for one taken out.
type Changes = BTreeMap<String, Option<(Cid, Vec<u8>)>>;

/// A writer's repo in a space, at their PDS, and how to ask for it.
struct Remote {
    space: String,
    did: String,
    pds: String,
    kem: String,
    /// `Atproto-Space-Credential <jwt>`, the group's to its own space.
    authorization: String,
    /// The key the credential is bound to.
    key: Keypair,
}

/// An answer to a request that asked for a MAC: the key share asked with,
/// the secret agreed with it, and the answer's headers and body.
struct Vouched {
    share: String,
    secret: [u8; 32],
    headers: reqwest::header::HeaderMap,
    body: Vec<u8>,
}

impl Vouched {
    /// An answer's header by its name in lower case.
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

impl Remote {
    async fn ask(&self, host: &Host, method: &str, params: &[(&str, &str)]) -> Result<Vouched> {
        let (share, secret) = encapsulate(&self.kem)?;
        let mut req = host
            .http
            .get(format!("{}/xrpc/com.atproto.space.{method}", self.pds))
            .query(&[("space", self.space.as_str()), ("repo", self.did.as_str())])
            .query(params)
            .header("accept-signature", accept_signature(COMMIT_MAC, &share));
        for (k, v) in sign_request(&self.key, &self.authorization, &self.did)? {
            req = req.header(k, v);
        }
        let res = req.send().await?.error_for_status()?;
        let headers = res.headers().clone();
        let body = crate::index_api::capped(res, REPO_AT_MOST).await;
        let body = body.context("an answer past the most a repo is read for")?;
        Ok(Vouched {
            share,
            secret,
            headers,
            body,
        })
    }

    /// The commit `answer`'s MAC vouches for, if it is this repo's.
    fn vouched(&self, answer: &Vouched) -> Result<Commit> {
        let commit = check_commit_mac(|n| answer.header(n), &answer.share, &answer.secret)?;
        let ours = commit.repo == self.did && commit.space == self.space;
        ensure!(ours, "a commit of another repo");
        Ok(commit)
    }
}

/// A commit as `listRepoOps` and `getLatestCommit` write it in JSON.
fn commit_of(json: &Json) -> Result<Commit> {
    let data = Value::from_json(&json["data"]);
    let Ok(Value::Bytes(data)) = data else {
        bail!("a commit with no data");
    };
    let text = |k: &str| {
        json[k]
            .as_str()
            .map(String::from)
            .context("a commit missing a field")
    };
    ensure!(json["ver"] == 1, "a commit of another version");
    Ok(Commit {
        repo: text("repo")?,
        space: text("space")?,
        rev: text("rev")?,
        data: data
            .as_slice()
            .try_into()
            .context("a commit's data is 32 bytes")?,
    })
}

/// A record's DRISL, if it is a map.
fn record_bytes(value: &Json) -> Option<Vec<u8>> {
    match Value::from_json(value) {
        Ok(v @ Value::Map(_)) => Some(drisl::encode(&v)),
        _ => None,
    }
}

impl Host {
    /// Marks `space` for the index to read, and wakes it.
    pub(crate) fn reindex(&self, space: &str) {
        self.indexing
            .lock()
            .expect("not poisoned")
            .insert(space.to_string());
        self.index_wake.notify_one();
    }

    /// Reads each space marked since the last time.
    pub async fn index_pending(&self) {
        let pending = std::mem::take(&mut *self.indexing.lock().expect("not poisoned"));
        for space in pending {
            let synced = self.sync_space(&space).await;
            tracing::info!(space, ?synced, "indexed");
        }
    }

    /// Brings the index of `space`, a group's here, to its writer set.
    pub async fn sync_space(&self, space: &str) -> Result<()> {
        let Some(hosted) = self.store.space(space)? else {
            return Ok(());
        };
        for writer in self.store.writers(space, None, u32::MAX)? {
            // One the space no longer lets write is no longer kept.
            if !self.may(space, &hosted, Some(&writer.did), Access::Write)? {
                self.store.unindex(space, &writer.did)?;
                continue;
            }
            let indexed = self.store.index_state(space, &writer.did)?;
            let at = indexed
                .as_ref()
                .map(|h| LtHash::from_state(&h.state).map(|s| s.digest()));
            if let Some(Ok(digest)) = at
                && digest.as_slice() == writer.data.as_slice()
            {
                continue;
            }
            let synced = self
                .sync_writer(space, &hosted.authority, &writer.did, indexed)
                .await;
            if let Err(e) = synced {
                tracing::warn!(space, repo = writer.did, error = %format!("{e:#}"), "a writer not indexed");
            }
        }
        Ok(())
    }

    async fn sync_writer(
        &self,
        space: &str,
        authority: &str,
        did: &str,
        indexed: Option<SpaceHead>,
    ) -> Result<()> {
        // A writer here: their repo is in the store.
        if self.store.account(did)?.is_some_and(|a| a.did == did) {
            let head = self
                .store
                .space_head(did, space)?
                .context("a writer with no repo")?;
            let records = self.store.space_records(did, space)?;
            let changes = records
                .into_iter()
                .map(|(path, bytes)| (path, Some((Cid::of_cbor(&bytes), bytes))))
                .collect();
            return self
                .store
                .index(space, did, &head, true, &changes, now_millis());
        }
        let remote = self.remote(space, authority, did).await?;
        if let Some(from) = indexed {
            match self.read_on(&remote, &from).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::info!(space, repo = did, error = %format!("{e:#}"), "reading the repo whole")
                }
            }
        }
        self.read_whole(&remote).await
    }

    /// `did`'s repo in `space`, at their PDS, with a credential to it the
    /// group `authority` issues itself.
    async fn remote(&self, space: &str, authority: &str, did: &str) -> Result<Remote> {
        let doc = self.identity.document(did).await?;
        let pds = did::pds(&doc).context("a writer's document names no PDS")?;
        let kem = did::kem_key(&doc).context("a writer with no #atproto_kem key")?;
        let key = Keypair::generate(Curve::P256);
        let signing = self
            .store
            .account(authority)?
            .context("the authority is here")?
            .signing;
        let kid = key.public().did_key();
        let make = Make {
            iss: authority,
            sub: space,
            key_id: Some(&kid),
            ..Make::default()
        };
        let now = now_millis() as u64 / 1000;
        let credential = space_auth::create(Kind::Credential, &make, &signing, now);
        let credential = credential?;
        Ok(Remote {
            space: space.into(),
            did: did.into(),
            pds,
            kem: kem.did_key(),
            authorization: format!("Atproto-Space-Credential {credential}"),
            key,
        })
    }

    /// Reads on from `from`: each page of the writes since, the set hash
    /// moved by each, checked against the commit they come to.
    async fn read_on(&self, remote: &Remote, from: &SpaceHead) -> Result<()> {
        let mut hash = LtHash::from_state(&from.state)?;
        let mut last: BTreeMap<String, Json> = BTreeMap::new();
        let mut cursor: Option<String> = None;
        let mut seen = HashSet::new();
        let (page, answer) = loop {
            let mut params = vec![("since", from.rev.as_str()), ("limit", PAGE)];
            if let Some(c) = &cursor {
                params.push(("cursor", c));
            }
            let answer = remote.ask(self, "listRepoOps", &params).await?;
            let page: Json = serde_json::from_slice(&answer.body)?;
            for op in page["ops"].as_array().context("a page with no ops")? {
                let (collection, rkey) = (
                    op["collection"]
                        .as_str()
                        .context("an op with no collection")?,
                    op["rkey"].as_str().context("an op with no rkey")?,
                );
                let cid_of = |k: &str| op[k].as_str().map(Cid::parse).transpose();
                if let Some(prev) = cid_of("prev")? {
                    hash.remove(&element(collection, rkey, &prev));
                }
                if let Some(cid) = cid_of("cid")? {
                    hash.add(&element(collection, rkey, &cid));
                }
                last.insert(format!("{collection}/{rkey}"), op.clone());
            }
            cursor = page["cursor"].as_str().map(String::from);
            // A server that answers a page it gave would be read forever.
            ensure!(seen.insert(cursor.clone()), "the oplog repeats a page");
            if cursor.is_none() {
                break (page, answer);
            }
        };
        // Every page carries the repo's head; the last one's is where the
        // oplog leads.
        let commit = remote.vouched(&answer)?;
        let written = commit_of(&page["commit"]).context("the oplog's head with no commit")?;
        ensure!(written == commit, "a commit not the one vouched for");
        ensure!(
            hash.digest() == commit.data,
            "the oplog does not come to its commit"
        );
        let mut changes = Changes::new();
        for (path, op) in last {
            let put = match op["cid"].as_str() {
                None => None,
                Some(cid) => {
                    // Its value as it is now: one written again since is
                    // another, and the repo is read whole instead.
                    let bytes = record_bytes(&op["value"]).context("a write with no record")?;
                    ensure!(
                        Cid::of_cbor(&bytes).to_string() == cid,
                        "a record not of its CID"
                    );
                    Some((Cid::of_cbor(&bytes), bytes))
                }
            };
            changes.insert(path, put);
        }
        let head = SpaceHead {
            rev: commit.rev,
            state: hash.state(),
        };
        self.store.index(
            &remote.space,
            &remote.did,
            &head,
            false,
            &changes,
            now_millis(),
        )
    }

    /// Reads the repo whole, from its STAR archive, checked against the
    /// commit it opens with.
    async fn read_whole(&self, remote: &Remote) -> Result<()> {
        let answer = remote.ask(self, "getRepo", &[]).await?;
        let (commit, records) = read_repo_star(
            &answer.body,
            &remote.space,
            &remote.did,
            &answer.share,
            &answer.secret,
            |n| answer.header(n),
        )?;
        let mut hash = LtHash::default();
        let mut changes = Changes::new();
        for (path, bytes) in records {
            let (collection, rkey) = path.split_once('/').expect("a record's path");
            let cid = Cid::of_cbor(&bytes);
            hash.add(&element(collection, rkey, &cid));
            // Hashed as it is, kept only if it is a record.
            match drisl::decode(&bytes) {
                Ok(Value::Map(_)) => {
                    changes.insert(path, Some((cid, bytes)));
                }
                _ => tracing::warn!(
                    space = remote.space,
                    repo = remote.did,
                    path,
                    "a record that is not a DRISL map, left out"
                ),
            }
        }
        // The archive's records came to its commit as it was read.
        let head = SpaceHead {
            rev: commit.rev,
            state: hash.state(),
        };
        self.store.index(
            &remote.space,
            &remote.did,
            &head,
            true,
            &changes,
            now_millis(),
        )
    }
}
