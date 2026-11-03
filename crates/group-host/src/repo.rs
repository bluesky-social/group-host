//! Groups' accounts and repos: making an account with its `did:plc`, and
//! changing its records a signed commit at a time.

use crate::Host;
use crate::store::{Account, Change, Head};
use crate::xrpc::XrpcError;
use anyhow::{Result, ensure};
use atproto::mst::{Entry, Node, Tree};
use atproto::plc::Op;
use atproto::{Cid, Commit, Curve, Keypair, Value, car, drisl, syntax};
use axum::http::StatusCode;
use std::collections::BTreeMap;

/// A change to one record.
#[derive(Debug, Clone, PartialEq)]
pub enum Write {
    /// A record at a path where there is none.
    Create {
        collection: String,
        rkey: String,
        value: serde_json::Value,
    },
    /// A record put where there is one.
    Update {
        collection: String,
        rkey: String,
        value: serde_json::Value,
    },
    Delete {
        collection: String,
        rkey: String,
    },
}

/// A commit made: its CID and revision.
#[derive(Debug, Clone, PartialEq)]
pub struct Committed {
    pub cid: Cid,
    pub rev: String,
}

fn invalid(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidRequest", message)
}

fn now() -> i64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.expect("after 1970").as_secs() as i64
}

impl Host {
    /// Makes an account at `handle`, one label under the host's handle
    /// domain: its keys, its `did:plc` published to the directory with
    /// the host's rotation key, and an empty repo. Its DID.
    pub async fn create_account(&self, handle: &str) -> Result<String, XrpcError> {
        let domain = &self.config.handle_domain;
        let label = handle.strip_suffix(domain.as_str()).unwrap_or_default();
        if label.is_empty() || label.contains('.') || !syntax::is_handle(handle) {
            return Err(XrpcError::new(
                StatusCode::BAD_REQUEST,
                "InvalidHandle",
                format!("a group's handle is one label under {domain}"),
            ));
        }
        let taken = || {
            XrpcError::new(
                StatusCode::BAD_REQUEST,
                "HandleNotAvailable",
                format!("{handle} is taken"),
            )
        };
        if self
            .store
            .account(handle)
            .map_err(XrpcError::internal)?
            .is_some()
        {
            return Err(taken());
        }
        let signing = Keypair::generate(Curve::K256);
        let rotation = [self.secrets.rotation.public().did_key()];
        let op = Op::atproto(
            &signing.public().did_key(),
            &rotation,
            handle,
            &self.config.public_url,
        );
        let genesis = op.sign(None, &self.secrets.rotation);
        let did = genesis.did();
        let failed = |e: anyhow::Error| {
            XrpcError::new(
                StatusCode::BAD_GATEWAY,
                "AccountCreationFailed",
                format!("{e:#}"),
            )
        };
        self.plc.publish(&did, &genesis).await.map_err(failed)?;
        let account = Account {
            did: did.clone(),
            handle: handle.to_string(),
            signing,
        };
        let head = self.sign(&account, &Tree::build(&BTreeMap::new()));
        // The handle checked above can still be taken as this was made:
        // the database's unique handle is what holds.
        let kept = self
            .store
            .create_account(&account, &head, now())
            .map_err(XrpcError::internal)?;
        kept.then_some(did).ok_or_else(taken)
    }

    /// `account`'s commit of `tree`, at the next revision.
    fn sign(&self, account: &Account, tree: &Tree) -> Head {
        let commit = Commit::sign(
            &account.did,
            tree.root(),
            &self.clock.next(),
            &account.signing,
        );
        Head {
            cid: commit.cid(),
            rev: commit.rev.clone(),
            block: commit.to_bytes(),
        }
    }

    /// Makes `writes` to `did`'s repo, as one commit.
    pub fn write(&self, did: &str, writes: &[Write]) -> Result<Committed, XrpcError> {
        let _one_at_a_time = self.writing.lock().expect("not poisoned");
        let account = self.store.account(did).map_err(XrpcError::internal)?;
        let account = account.ok_or_else(|| {
            XrpcError::new(
                StatusCode::BAD_REQUEST,
                "RepoNotFound",
                format!("no repo {did} here"),
            )
        })?;
        let head = self
            .store
            .head(&account.did)
            .map_err(XrpcError::internal)?
            .expect("an account has a repo");
        let mut paths = self
            .store
            .paths(&account.did)
            .map_err(XrpcError::internal)?;
        let mut changes: Vec<Change> = Vec::new();
        for write in writes {
            let (collection, rkey, value, exists) = match write {
                Write::Create {
                    collection,
                    rkey,
                    value,
                } => (collection, rkey, Some(value), false),
                Write::Update {
                    collection,
                    rkey,
                    value,
                } => (collection, rkey, Some(value), true),
                Write::Delete { collection, rkey } => (collection, rkey, None, true),
            };
            if !syntax::is_nsid(collection) || !syntax::is_record_key(rkey) {
                return Err(invalid(format!(
                    "{collection}/{rkey} is not a record's path"
                )));
            }
            let path = format!("{collection}/{rkey}");
            if paths.contains_key(&path) != exists {
                let what = if exists {
                    "no record at"
                } else {
                    "a record already at"
                };
                return Err(invalid(format!("there is {what} {path}")));
            }
            let put = value.map(|v| record(collection, v)).transpose()?;
            match &put {
                Some((cid, _)) => paths.insert(path, *cid),
                None => paths.remove(&path),
            };
            changes.push((collection.clone(), rkey.clone(), put));
        }
        let new = self.sign(&account, &Tree::build(&paths));
        let moved = self.store.commit(&account.did, &head.cid, &new, &changes);
        moved
            .and_then(|moved| {
                ensure!(moved, "{did}'s repo moved under a write");
                Ok(())
            })
            .map_err(XrpcError::internal)?;
        Ok(Committed {
            cid: new.cid,
            rev: new.rev,
        })
    }

    /// `did`'s whole repo as a CAR file: its commit, then each node of its
    /// tree, parents first, with the records it holds in the order it
    /// holds them.
    pub fn export(&self, did: &str) -> Result<Option<Vec<u8>>> {
        let Some(head) = self.store.head(did)? else {
            return Ok(None);
        };
        let records: BTreeMap<Cid, Vec<u8>> = self
            .store
            .records(did)?
            .into_iter()
            .map(|r| (r.cid, r.value))
            .collect();
        let tree = Tree::build(&self.store.paths(did)?);
        let mut blocks: Vec<(Cid, &[u8])> = vec![(head.cid, &head.block)];
        walk(tree.root_node(), &records, &mut blocks);
        Ok(Some(car::write(
            &[head.cid],
            blocks.iter().map(|(c, b)| (c, *b)),
        )))
    }
}

fn walk<'a>(
    node: &'a Node,
    records: &'a BTreeMap<Cid, Vec<u8>>,
    blocks: &mut Vec<(Cid, &'a [u8])>,
) {
    blocks.push((node.cid(), node.bytes()));
    for entry in node.entries() {
        match entry {
            Entry::Tree(n) => walk(n, records, blocks),
            Entry::Leaf { value, .. } => blocks.push((*value, &records[value])),
        }
    }
}

/// A record's JSON as kept: its CID and DRISL. It must be a record by the
/// data model, of its collection's `$type`.
fn record(collection: &str, value: &serde_json::Value) -> Result<(Cid, Vec<u8>), XrpcError> {
    drisl::check_record(value).map_err(|e| invalid(e.to_string()))?;
    if value.get("$type").and_then(serde_json::Value::as_str) != Some(collection) {
        return Err(invalid(format!(
            "a record in {collection} has $type {collection}"
        )));
    }
    let bytes = drisl::encode(&Value::from_json(value).expect("checked as a record"));
    Ok((Cid::of_cbor(&bytes), bytes))
}
