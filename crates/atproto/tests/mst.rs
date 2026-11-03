//! The MST, against atproto's interop test vectors (tests/interop/mst and
//! tests/interop/firehose), and every tree it refuses to read.

use atproto::cid::Cid;
use atproto::drisl::{self, Value};
use atproto::mst::{Blocks, Tree, common_prefix, depth};
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};

fn vectors(path: &str) -> Vec<Json> {
    let path = format!("{}/tests/interop/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn s(v: &Json) -> &str {
    v.as_str().unwrap()
}

#[test]
fn depths_and_prefixes_as_the_vectors_say() {
    for v in vectors("mst/key_heights.json") {
        assert_eq!(
            depth(s(&v["key"]).as_bytes()),
            v["height"].as_u64().unwrap() as u32
        );
    }
    for v in vectors("mst/common_prefix.json") {
        let len = common_prefix(s(&v["left"]).as_bytes(), s(&v["right"]).as_bytes());
        assert_eq!(len, v["len"].as_u64().unwrap() as usize);
    }
}

#[test]
fn an_empty_tree_is_one_empty_node() {
    let tree = Tree::build(&BTreeMap::new());
    assert_eq!(
        tree.root().to_string(),
        "bafyreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm"
    );
    assert!(tree.leaves().is_empty());
    assert!(tree.root_node().entries().is_empty());
    assert_eq!(tree.root_node().cid(), tree.root());
    assert_eq!(tree.root_node().bytes(), b"\xa2ae\x80al\xf6");
    // A key's proof in an empty tree is the one node.
    assert_eq!(tree.covering_proof("a/b").len(), 1);
}

#[test]
fn commits_and_proofs_as_the_vectors_say() {
    for v in vectors("firehose/commit-proof-fixtures.json") {
        let leaf = Cid::parse(s(&v["leafValue"])).unwrap();
        let keys = |k: &Json| {
            k.as_array()
                .unwrap()
                .iter()
                .map(|k| s(k).to_string())
                .collect::<Vec<_>>()
        };
        let mut records: BTreeMap<String, Cid> =
            keys(&v["keys"]).into_iter().map(|k| (k, leaf)).collect();
        let before = Tree::build(&records);
        assert_eq!(
            before.root().to_string(),
            s(&v["rootBeforeCommit"]),
            "{}",
            v["comment"]
        );

        let (adds, dels) = (keys(&v["adds"]), keys(&v["dels"]));
        for k in &adds {
            records.insert(k.clone(), leaf);
        }
        for k in &dels {
            records.remove(k);
        }
        let after = Tree::build(&records);
        assert_eq!(
            after.root().to_string(),
            s(&v["rootAfterCommit"]),
            "{}",
            v["comment"]
        );

        let mut proof = after.new_since(&before);
        for k in adds.iter().chain(&dels) {
            proof.extend(after.covering_proof(k));
        }
        let got: BTreeSet<String> = proof.keys().map(Cid::to_string).collect();
        let want: BTreeSet<String> = keys(&v["blocksInProof"]).into_iter().collect();
        assert_eq!(got, want, "{}", v["comment"]);
    }
}

#[test]
fn reads_back_what_it_built() {
    let path = format!(
        "{}/tests/interop/mst/example_keys.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let records: BTreeMap<String, Cid> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|k| (k.to_string(), Cid::of_cbor(k.as_bytes())))
        .collect();
    let tree = Tree::build(&records);
    assert!(tree.nodes().len() > 1);
    let read = Tree::read(tree.root(), &tree.blocks()).unwrap();
    assert_eq!(read, tree);
    let leaves: BTreeMap<String, Cid> = read
        .leaves()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    assert_eq!(leaves, records);
}

/// A node's block, as `entries` (each `[p, k, v, t]`) and `l` make it.
fn node(l: Value, entries: Vec<Value>) -> (Cid, Vec<u8>) {
    let bytes = drisl::encode(&Value::Map(BTreeMap::from([
        ("l".into(), l),
        ("e".into(), Value::Array(entries)),
    ])));
    (Cid::of_cbor(&bytes), bytes)
}

fn entry(p: i64, k: &[u8], v: Cid, t: Value) -> Value {
    Value::Map(BTreeMap::from([
        ("p".into(), Value::Integer(p)),
        ("k".into(), Value::Bytes(k.to_vec())),
        ("v".into(), Value::Link(v)),
        ("t".into(), t),
    ]))
}

fn refused(root: Cid, blocks: &[(Cid, Vec<u8>)]) -> String {
    let blocks: Blocks = blocks.iter().cloned().collect();
    Tree::read(root, &blocks).unwrap_err().to_string()
}

#[test]
fn refuses_every_tree_but_the_one_its_keys_make() {
    let v = Cid::of_cbor(b"\xa0");
    let block = |value: Value| {
        let bytes = drisl::encode(&value);
        (Cid::of_cbor(&bytes), bytes)
    };
    let (missing, _) = node(Value::Null, vec![]);
    assert_eq!(refused(missing, &[]), format!("node {missing} is missing"));
    let raw = Cid::of_raw(b"x");
    assert_eq!(refused(raw, &[]), format!("node {raw} is not DAG-CBOR"));

    let garbage = (Cid::of_cbor(&[0xff]), vec![0xff]);
    assert!(
        refused(garbage.0, std::slice::from_ref(&garbage))
            .starts_with(&format!("node {}: ", garbage.0))
    );

    let not_node = |b: (Cid, Vec<u8>)| {
        assert_eq!(
            refused(b.0, std::slice::from_ref(&b)),
            format!("node {} is not a tree node", b.0)
        );
    };
    not_node(block(Value::Integer(1)));
    not_node(block(Value::Map(BTreeMap::from([(
        "e".into(),
        Value::Array(vec![]),
    )]))));
    not_node(node(Value::Integer(1), vec![]));
    not_node(node(Value::Null, vec![Value::Integer(1)]));
    not_node(node(Value::Null, vec![Value::Map(BTreeMap::new())]));
    not_node(node(Value::Null, vec![entry(-1, b"a/b", v, Value::Null)]));
    not_node(node(Value::Null, vec![entry(1, b"a/b", v, Value::Null)]));
    not_node(node(Value::Null, vec![entry(0, &[0xff], v, Value::Null)]));

    // Keys at the wrong depth: two keys at depth 0 split across a node and
    // its subtree, where they belong in one.
    let (sub, sub_bytes) = node(Value::Null, vec![entry(0, b"g", v, Value::Null)]);
    let (top, top_bytes) = node(Value::Link(sub), vec![entry(0, b"h", v, Value::Null)]);
    assert_eq!(depth(b"g"), 0);
    assert_eq!(depth(b"h"), 0);
    assert_eq!(
        refused(top, &[(sub, sub_bytes), (top, top_bytes)]),
        "the tree is not the one its keys make"
    );

    // A chain of nodes deeper than any tree of real keys.
    let mut chain = vec![node(Value::Null, vec![entry(0, b"a", v, Value::Null)])];
    for _ in 0..65 {
        let below = chain.last().unwrap().0;
        chain.push(node(Value::Link(below), vec![]));
    }
    assert_eq!(
        refused(chain.last().unwrap().0, &chain),
        "the tree is too deep"
    );
}
