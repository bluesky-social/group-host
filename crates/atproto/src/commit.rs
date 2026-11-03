//! A repo's signed commit (version 3): the account, the root of its
//! records' tree, a revision, and the account's signature over the rest.

use crate::cid::Cid;
use crate::drisl::{self, Value};
use crate::keys::{Keypair, PublicKey};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub did: String,
    /// The root of the records' tree.
    pub data: Cid,
    /// A TID, later than every commit's before it.
    pub rev: String,
    /// Kept for version 2's sake, and almost always none.
    pub prev: Option<Cid>,
    pub sig: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitError(pub &'static str);

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for CommitError {}

impl Commit {
    /// The commit of `did`'s tree `data` at `rev`, signed with `key`.
    pub fn sign(did: &str, data: Cid, rev: &str, key: &Keypair) -> Commit {
        let mut commit = Commit {
            did: did.to_string(),
            data,
            rev: rev.to_string(),
            prev: None,
            sig: Vec::new(),
        };
        commit.sig = key.sign(&drisl::encode(&commit.unsigned())).to_vec();
        commit
    }

    fn unsigned(&self) -> Value {
        Value::Map(self.fields())
    }

    fn fields(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("did".into(), Value::String(self.did.clone())),
            ("version".into(), Value::Integer(3)),
            ("data".into(), Value::Link(self.data)),
            ("rev".into(), Value::String(self.rev.clone())),
            ("prev".into(), self.prev.map_or(Value::Null, Value::Link)),
        ])
    }

    /// The signed commit, as its block holds it.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut map = self.fields();
        map.insert("sig".into(), Value::Bytes(self.sig.clone()));
        drisl::encode(&Value::Map(map))
    }

    pub fn cid(&self) -> Cid {
        Cid::of_cbor(&self.to_bytes())
    }

    /// The commit a block holds: version 3, with every field it needs.
    pub fn from_bytes(bytes: &[u8]) -> Result<Commit, CommitError> {
        let bad = CommitError("not a version 3 commit");
        let Ok(Value::Map(map)) = drisl::decode(bytes) else {
            return Err(bad);
        };
        let field = |k: &str| map.get(k);
        let (
            Some(Value::String(did)),
            Some(Value::Integer(3)),
            Some(Value::Link(data)),
            Some(Value::String(rev)),
            Some(prev),
            Some(Value::Bytes(sig)),
        ) = (
            field("did"),
            field("version"),
            field("data"),
            field("rev"),
            field("prev"),
            field("sig"),
        )
        else {
            return Err(bad);
        };
        let prev = match prev {
            Value::Null => None,
            Value::Link(c) => Some(*c),
            _ => return Err(bad),
        };
        Ok(Commit {
            did: did.clone(),
            data: *data,
            rev: rev.clone(),
            prev,
            sig: sig.clone(),
        })
    }

    /// Whether `key`, the account's signing key, signed this.
    pub fn verify(&self, key: &PublicKey) -> bool {
        key.verify(&drisl::encode(&self.unsigned()), &self.sig)
    }
}
