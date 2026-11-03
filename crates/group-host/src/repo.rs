//! Groups' accounts and repos: making an account with its `did:plc`, and
//! changing its records a signed commit at a time.

use crate::Host;
use crate::blobs::blob_refs;
use crate::store::{Account, Change, Event, Head};
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use anyhow::{Context, Result};
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

impl Write {
    /// Where it writes: its collection and record key.
    pub fn path(&self) -> (&str, &str) {
        match self {
            Write::Create {
                collection, rkey, ..
            }
            | Write::Update {
                collection, rkey, ..
            }
            | Write::Delete { collection, rkey } => (collection, rkey),
        }
    }
}

/// A write checked against the records it changes: its path
/// (`collection/rkey`), the record it puts as its CID and DRISL (none for
/// a delete), and the blobs that record references.
pub(crate) struct Checked {
    pub path: String,
    pub put: Option<(Cid, Vec<u8>)>,
    pub blobs: Vec<Cid>,
}

/// A commit made: its CID and revision.
#[derive(Debug, Clone, PartialEq)]
pub struct Committed {
    pub cid: Cid,
    pub rev: String,
}

pub(crate) fn invalid(message: impl Into<String>) -> XrpcError {
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
        // A handle names one account whatever its case: kept in lower case.
        let handle = handle.to_ascii_lowercase();
        let handle = handle.as_str();
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
        let kem = Keypair::generate(Curve::P256);
        let rotation = [self.secrets.rotation.public().did_key()];
        let mut op = Op::atproto(
            &signing.public().did_key(),
            &rotation,
            handle,
            &self.config.public_url,
        );
        op.verification_methods
            .insert("atproto_kem".into(), kem.public().did_key());
        let genesis = op.sign(None, &self.secrets.rotation);
        let did = genesis.did();
        let failed = |e: anyhow::Error| {
            XrpcError::new(
                StatusCode::BAD_GATEWAY,
                "AccountCreationFailed",
                format!("{e:#}"),
            )
        };
        // Published before the account is kept: if keeping it fails, or
        // another took the handle meanwhile, the DID stays in the directory
        // with no account here behind it.
        self.plc.publish(&did, &genesis).await.map_err(failed)?;
        let account = Account {
            did: did.clone(),
            handle: handle.to_string(),
            signing,
            kem,
        };
        let tree = Tree::build(&BTreeMap::new());
        let head = self.sign(&account, &tree);
        let at = Value::String(iso(now_millis()));
        let events = [
            event(
                "#identity",
                [
                    ("did", Value::String(did.clone())),
                    ("handle", Value::String(handle.into())),
                    ("time", at.clone()),
                ],
            ),
            event(
                "#account",
                [
                    ("did", Value::String(did.clone())),
                    ("active", Value::Bool(true)),
                    ("time", at),
                ],
            ),
            commit_event(
                &did,
                &head,
                None,
                None,
                Vec::new(),
                Vec::new(),
                tree.blocks(),
            ),
        ];
        // The handle checked above can still be taken as this was made:
        // the database's unique handle is what holds.
        let kept = self
            .store
            .create_account(&account, &head, now(), &events)
            .map_err(XrpcError::internal)?;
        self.announce();
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
        let before = Tree::build(&paths);
        let mut changes: Vec<Change> = Vec::new();
        let mut ops = Vec::new();
        let mut new_blobs = Vec::new();
        let mut blocks = BTreeMap::new();
        for write in writes {
            let Checked { path, put, blobs } = self.check(&account.did, write, &paths)?;
            new_blobs.extend(blobs.iter().copied());
            let prev = match &put {
                Some((cid, bytes)) => {
                    blocks.insert(*cid, bytes.clone());
                    paths.insert(path.clone(), *cid)
                }
                None => paths.remove(&path),
            };
            ops.push(op(path, put.as_ref().map(|(cid, _)| *cid), prev));
            let (collection, rkey) = write.path();
            changes.push(Change {
                collection: collection.into(),
                rkey: rkey.into(),
                put,
                blobs,
            });
        }
        let tree = Tree::build(&paths);
        let new = self.sign(&account, &tree);
        // What a reader needs to check the commit against the repo before
        // it: the new nodes, and each changed path's covering proof.
        blocks.extend(tree.new_since(&before));
        for c in &changes {
            blocks.extend(tree.covering_proof(&format!("{}/{}", c.collection, c.rkey)));
        }
        let previous = Commit::from_bytes(&head.block).expect("a commit the host wrote");
        let events = [commit_event(
            &account.did,
            &new,
            Some(&head.rev),
            Some(previous.data),
            ops,
            new_blobs,
            blocks,
        )];
        let moved = self
            .store
            .commit(&account.did, &head.cid, &new, &changes, &events);
        let raced = format!("{did}'s repo moved under a write");
        let orphans = moved
            .and_then(|moved| moved.context(raced))
            .map_err(XrpcError::internal)?;
        self.remove_blobs(orphans.iter().map(|cid| (account.did.as_str(), cid)));
        self.announce();
        Ok(Committed {
            cid: new.cid,
            rev: new.rev,
        })
    }

    /// `write` to `did`'s records, now at `paths`, checked: its path is a
    /// record's, a create's free and an update's or delete's taken, its
    /// record one by the data model of its collection's `$type`, and each
    /// blob it references uploaded to `did`.
    pub(crate) fn check(
        &self,
        did: &str,
        write: &Write,
        paths: &BTreeMap<String, Cid>,
    ) -> Result<Checked, XrpcError> {
        let (collection, rkey) = write.path();
        let (value, exists) = match write {
            Write::Create { value, .. } => (Some(value), false),
            Write::Update { value, .. } => (Some(value), true),
            Write::Delete { .. } => (None, true),
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
        // A record may reference only blobs uploaded to its account.
        let blobs = value.map(blob_refs).unwrap_or_default();
        for cid in &blobs {
            let blob = self.store.blob(did, cid).map_err(XrpcError::internal)?;
            if blob.is_none() {
                return Err(invalid(format!("blob {cid} was not uploaded to {did}")));
            }
        }
        Ok(Checked { path, put, blobs })
    }

    /// Wakes the firehose's streams, and tells the relays in
    /// `PDS_CRAWLERS` this host has news: at most every twenty minutes,
    /// and the first time after it starts, so a relay that marked it
    /// offline comes back.
    fn announce(&self) {
        let _ = self.events.send(());
        let mut last = self.crawled.lock().expect("not poisoned");
        if last.is_some_and(|at| at.elapsed() < CRAWL_EVERY) {
            return;
        }
        *last = Some(std::time::Instant::now());
        let hostname = self
            .config
            .public_url
            .split("://")
            .nth(1)
            .unwrap_or_default()
            .to_string();
        for crawler in self.config.crawlers.clone() {
            let body = serde_json::json!({ "hostname": hostname });
            let send = self
                .http
                .post(format!("{crawler}/xrpc/com.atproto.sync.requestCrawl"))
                .json(&body)
                .send();
            tokio::spawn(async move {
                let said = send.await.and_then(|r| r.error_for_status());
                if let Err(e) = said {
                    tracing::warn!(crawler, error = %e, "requestCrawl failed");
                }
            });
        }
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

/// How long between telling the relays of news.
const CRAWL_EVERY: std::time::Duration = std::time::Duration::from_secs(20 * 60);

/// An event of `kind` with the fields given.
fn event<const N: usize>(kind: &'static str, fields: [(&str, Value); N]) -> Event {
    let map = fields
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    Event {
        kind,
        body: drisl::encode(&Value::Map(map)),
    }
}

/// A commit's op: what changed at `path`, as the firehose has it.
fn op(path: String, cid: Option<Cid>, prev: Option<Cid>) -> Value {
    let action = match (cid, prev) {
        (Some(_), None) => "create",
        (Some(_), Some(_)) => "update",
        (None, _) => "delete",
    };
    let mut map = BTreeMap::from([
        ("action".to_string(), Value::String(action.into())),
        ("path".to_string(), Value::String(path)),
        ("cid".to_string(), cid.map_or(Value::Null, Value::Link)),
    ]);
    if let Some(prev) = prev {
        map.insert("prev".into(), Value::Link(prev));
    }
    Value::Map(map)
}

/// A `#commit` of `head`: its ops, and a CAR of `blocks` and the commit.
fn commit_event(
    did: &str,
    head: &Head,
    since: Option<&str>,
    prev_data: Option<Cid>,
    ops: Vec<Value>,
    blobs: Vec<Cid>,
    mut blocks: BTreeMap<Cid, Vec<u8>>,
) -> Event {
    blocks.insert(head.cid, head.block.clone());
    let car = car::write(&[head.cid], blocks.iter().map(|(c, b)| (c, b.as_slice())));
    let mut fields = vec![
        ("repo", Value::String(did.into())),
        ("commit", Value::Link(head.cid)),
        ("rev", Value::String(head.rev.clone())),
        (
            "since",
            since.map_or(Value::Null, |s| Value::String(s.into())),
        ),
        ("blocks", Value::Bytes(car)),
        ("ops", Value::Array(ops)),
        (
            "blobs",
            Value::Array(blobs.into_iter().map(Value::Link).collect()),
        ),
        ("rebase", Value::Bool(false)),
        ("tooBig", Value::Bool(false)),
        ("time", Value::String(iso(now_millis()))),
    ];
    if let Some(data) = prev_data {
        fields.push(("prevData", Value::Link(data)));
    }
    let map = fields
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    Event {
        kind: "#commit",
        body: drisl::encode(&Value::Map(map)),
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
