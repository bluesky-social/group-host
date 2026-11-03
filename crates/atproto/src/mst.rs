//! The Merkle search tree a repo's records hang from: keys
//! (`collection/rkey`) to the CIDs of their records, in a tree whose shape
//! follows from the keys alone, so that the same records always hash to
//! the same root.
//!
//! A tree is built whole from its keys ([`Tree::build`]) rather than
//! changed in place. That is all a group's repo needs while repos are
//! small; changing it in place is for when they are not.

use crate::cid::{Cid, DAG_CBOR};
use crate::drisl::{self, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MstError(pub String);

impl fmt::Display for MstError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MstError {}

fn err<T>(what: impl Into<String>) -> Result<T, MstError> {
    Err(MstError(what.into()))
}

/// The layer a key sits at: the leading zero bits of its SHA-256, in
/// pairs, so that each layer holds about a quarter of the one below.
pub fn depth(key: &[u8]) -> u32 {
    let hash = Sha256::digest(key);
    let mut zeros = 0;
    for byte in hash {
        zeros += byte.leading_zeros();
        if byte != 0 {
            break;
        }
    }
    zeros / 2
}

/// How many bytes `a` and `b` start with in common.
pub fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// A part of a node, in key order: a subtree of the keys between its
/// neighbours, or a key and its record's CID.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Tree(Node),
    Leaf { key: String, value: Cid },
}

/// A node of the tree, with its bytes and CID worked out once.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    entries: Vec<Entry>,
    bytes: Vec<u8>,
    cid: Cid,
}

impl Node {
    fn new(entries: Vec<Entry>) -> Node {
        let bytes = drisl::encode(&Node::value(&entries));
        Node {
            cid: Cid::of_cbor(&bytes),
            entries,
            bytes,
        }
    }

    /// The node as DRISL has it: `l`, its leftmost subtree, then `e`, its
    /// keys, each with its prefix shared with the key before it elided, and
    /// `t`, the subtree to its right.
    fn value(entries: &[Entry]) -> Value {
        let link = |e: Option<&Entry>| match e {
            Some(Entry::Tree(n)) => Value::Link(n.cid),
            _ => Value::Null,
        };
        let mut e = Vec::new();
        let mut last: &[u8] = &[];
        for (i, entry) in entries.iter().enumerate() {
            if let Entry::Leaf { key, value } = entry {
                let p = common_prefix(last, key.as_bytes());
                e.push(Value::Map(BTreeMap::from([
                    ("p".into(), Value::Integer(p as i64)),
                    ("k".into(), Value::Bytes(key.as_bytes()[p..].to_vec())),
                    ("v".into(), Value::Link(*value)),
                    ("t".into(), link(entries.get(i + 1))),
                ])));
                last = key.as_bytes();
            }
        }
        Value::Map(BTreeMap::from([
            ("l".into(), link(entries.first())),
            ("e".into(), Value::Array(e)),
        ]))
    }

    pub fn cid(&self) -> Cid {
        self.cid
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The index of the first key at or after `key`, or past the end.
    fn index_at_or_after(&self, key: &str) -> usize {
        self.entries
            .iter()
            .position(|e| matches!(e, Entry::Leaf { key: k, .. } if k.as_str() >= key))
            .unwrap_or(self.entries.len())
    }

    fn tree_at(&self, index: usize) -> Option<&Node> {
        match self.entries.get(index) {
            Some(Entry::Tree(n)) => Some(n),
            _ => None,
        }
    }

    fn before(&self, index: usize) -> Option<&Node> {
        index.checked_sub(1).and_then(|i| self.tree_at(i))
    }

    fn add_to(&self, blocks: &mut Blocks) {
        blocks.insert(self.cid, self.bytes.clone());
    }

    // What a reader needs to see `key`'s place in the tree, as the
    // reference implementation gives it, so that the firehose's proofs are
    // the same: the nodes down to where the key is or would be, and down
    // the edges of the subtrees either side of it.

    fn proof_for_key(&self, key: &str, blocks: &mut Blocks) {
        let index = self.index_at_or_after(key);
        let found = matches!(self.entries.get(index), Some(Entry::Leaf { key: k, .. }) if k == key);
        if !found {
            match self.before(index) {
                Some(prev) => prev.proof_for_key(key, blocks),
                None => return,
            }
        }
        self.add_to(blocks);
    }

    fn proof_for_left(&self, key: &str, blocks: &mut Blocks) {
        let index = self.index_at_or_after(key);
        if let Some(prev) = self.before(index) {
            prev.proof_for_left(key, blocks);
        }
        self.add_to(blocks);
    }

    fn proof_for_right(&self, key: &str, blocks: &mut Blocks) {
        let index = self.index_at_or_after(key);
        let at = match self.entries.get(index) {
            Some(e) => Some((index, e)),
            None => index.checked_sub(1).map(|i| (i, &self.entries[i])),
        };
        // Its neighbour from `index`, not from where it was found, as the
        // reference has it: past the end, that is the key itself, and so no
        // subtree.
        let next = match at {
            Some((_, Entry::Tree(n))) => Some(n),
            Some((_, Entry::Leaf { key: k, .. })) if k == key => self.tree_at(index + 1),
            Some((_, Entry::Leaf { .. })) => self.before(index),
            None => None,
        };
        if let Some(n) = next {
            n.proof_for_right(key, blocks);
        }
        self.add_to(blocks);
    }

    fn walk<'a>(&'a self, nodes: &mut Vec<&'a Node>, leaves: &mut Vec<(&'a str, Cid)>) {
        nodes.push(self);
        for e in &self.entries {
            match e {
                Entry::Tree(n) => n.walk(nodes, leaves),
                Entry::Leaf { key, value } => leaves.push((key, *value)),
            }
        }
    }
}

/// Blocks by CID: a node's, a record's, a commit's.
pub type Blocks = BTreeMap<Cid, Vec<u8>>;

/// A whole tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    root: Node,
}

impl Tree {
    /// The tree of `records`, keys to their records' CIDs.
    pub fn build(records: &BTreeMap<String, Cid>) -> Tree {
        let items: Vec<(&str, Cid, u32)> = records
            .iter()
            .map(|(k, v)| (k.as_str(), *v, depth(k.as_bytes())))
            .collect();
        let top = items.iter().map(|i| i.2).max().unwrap_or(0);
        Tree {
            root: build(&items, top),
        }
    }

    /// The root's CID: a commit's `data`.
    pub fn root(&self) -> Cid {
        self.root.cid
    }

    pub fn root_node(&self) -> &Node {
        &self.root
    }

    /// Every node, parents before children, in key order.
    pub fn nodes(&self) -> Vec<&Node> {
        let (mut nodes, mut leaves) = (Vec::new(), Vec::new());
        self.root.walk(&mut nodes, &mut leaves);
        nodes
    }

    /// Every key and its record's CID, in key order.
    pub fn leaves(&self) -> Vec<(&str, Cid)> {
        let (mut nodes, mut leaves) = (Vec::new(), Vec::new());
        self.root.walk(&mut nodes, &mut leaves);
        leaves
    }

    /// Every node's block.
    pub fn blocks(&self) -> Blocks {
        self.nodes()
            .into_iter()
            .map(|n| (n.cid, n.bytes.clone()))
            .collect()
    }

    /// The nodes of this tree that `before` does not have: what a commit
    /// from `before` to this adds.
    pub fn new_since(&self, before: &Tree) -> Blocks {
        let had: BTreeSet<Cid> = before.nodes().into_iter().map(|n| n.cid).collect();
        self.nodes()
            .into_iter()
            .filter(|n| !had.contains(&n.cid))
            .map(|n| (n.cid, n.bytes.clone()))
            .collect()
    }

    /// The nodes a reader needs, beside the new ones, to undo a change to
    /// `key` and find the tree before: the nodes down to its place, and
    /// down the edges of its neighbours.
    pub fn covering_proof(&self, key: &str) -> Blocks {
        let mut blocks = Blocks::new();
        self.root.proof_for_key(key, &mut blocks);
        self.root.proof_for_left(key, &mut blocks);
        self.root.proof_for_right(key, &mut blocks);
        blocks
    }

    /// The tree whose root is `root`, read from `blocks`, every node
    /// checked: keys in order, each at its depth, and the whole the one
    /// tree its keys make. A node missing from `blocks` fails it.
    pub fn read(root: Cid, blocks: &Blocks) -> Result<Tree, MstError> {
        let mut records = BTreeMap::new();
        read_node(root, blocks, &mut records, 0)?;
        let tree = Tree::build(&records);
        if tree.root() != root {
            return err("the tree is not the one its keys make");
        }
        Ok(tree)
    }
}

fn build(items: &[(&str, Cid, u32)], height: u32) -> Node {
    let mut entries = Vec::new();
    let mut run = 0;
    for (i, &(key, value, d)) in items.iter().enumerate() {
        if d == height {
            if run < i {
                entries.push(Entry::Tree(build(&items[run..i], height - 1)));
            }
            entries.push(Entry::Leaf {
                key: key.to_string(),
                value,
            });
            run = i + 1;
        }
    }
    if run < items.len() {
        entries.push(Entry::Tree(build(&items[run..], height - 1)));
    }
    Node::new(entries)
}

/// How deep a tree read from elsewhere may go.
const MAX_LAYERS: usize = 64;

fn read_node(
    cid: Cid,
    blocks: &Blocks,
    records: &mut BTreeMap<String, Cid>,
    layer: usize,
) -> Result<(), MstError> {
    if layer > MAX_LAYERS {
        return err("the tree is too deep");
    }
    if cid.codec() != DAG_CBOR {
        return err(format!("node {cid} is not DAG-CBOR"));
    }
    let Some(bytes) = blocks.get(&cid) else {
        return err(format!("node {cid} is missing"));
    };
    let value = drisl::decode(bytes).or_else(|e| err(format!("node {cid}: {e}")))?;
    let bad = || MstError(format!("node {cid} is not a tree node"));
    let Value::Map(map) = value else {
        return Err(bad());
    };
    let (Some(Value::Array(entries)), Some(left), 2) = (map.get("e"), map.get("l"), map.len())
    else {
        return Err(bad());
    };
    let subtree = |v: &Value, records: &mut BTreeMap<String, Cid>| match v {
        Value::Null => Ok(()),
        Value::Link(c) => read_node(*c, blocks, records, layer + 1),
        _ => Err(bad()),
    };
    subtree(left, records)?;
    let mut last: Vec<u8> = Vec::new();
    for e in entries {
        let Value::Map(e) = e else {
            return Err(bad());
        };
        let (Some(Value::Integer(p)), Some(Value::Bytes(k)), Some(Value::Link(v)), Some(t), 4) =
            (e.get("p"), e.get("k"), e.get("v"), e.get("t"), e.len())
        else {
            return Err(bad());
        };
        let p = usize::try_from(*p).map_err(|_| bad())?;
        if p > last.len() {
            return Err(bad());
        }
        let key = [&last[..p], k].concat();
        let key = String::from_utf8(key).map_err(|_| bad())?;
        records.insert(key.clone(), *v);
        subtree(t, records)?;
        last = key.into_bytes();
    }
    Ok(())
}
