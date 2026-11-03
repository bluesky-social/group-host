//! CAR files (CARv1): a header naming the roots, then blocks, each its
//! CID and bytes. How a repo is exported, imported, and sent down the
//! firehose a commit at a time.

use crate::cid::{Cid, DAG_CBOR};
use crate::drisl::{self, Value};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarError(pub String);

impl fmt::Display for CarError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CarError {}

fn err<T>(what: impl Into<String>) -> Result<T, CarError> {
    Err(CarError(what.into()))
}

/// A CAR file read: its roots, and its blocks by CID.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Car {
    pub roots: Vec<Cid>,
    pub blocks: BTreeMap<Cid, Vec<u8>>,
}

fn varint(mut n: u64, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

/// A CAR file of `roots` and `blocks`, the blocks in the order given.
pub fn write<'a>(roots: &[Cid], blocks: impl IntoIterator<Item = (&'a Cid, &'a [u8])>) -> Vec<u8> {
    let header = drisl::encode(&Value::Map(BTreeMap::from([
        (
            "roots".into(),
            Value::Array(roots.iter().map(|c| Value::Link(*c)).collect()),
        ),
        ("version".into(), Value::Integer(1)),
    ])));
    let mut out = Vec::new();
    varint(header.len() as u64, &mut out);
    out.extend(header);
    for (cid, bytes) in blocks {
        let cid = cid.to_bytes();
        varint((cid.len() + bytes.len()) as u64, &mut out);
        out.extend(cid);
        out.extend(bytes);
    }
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn varint(&mut self) -> Result<usize, CarError> {
        let mut n: u64 = 0;
        for shift in (0..64).step_by(7) {
            let Some(&b) = self.bytes.get(self.at) else {
                return err("the file ends in the middle of a length");
            };
            self.at += 1;
            n |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                // Past what this machine can hold, `take` refuses it.
                return Ok(usize::try_from(n).unwrap_or(usize::MAX));
            }
        }
        err("a length too long")
    }

    fn take(&mut self, n: usize) -> Result<&[u8], CarError> {
        if n > self.bytes.len() - self.at {
            return err("the file ends in the middle of a block");
        }
        let start = self.at;
        self.at += n;
        Ok(&self.bytes[start..self.at])
    }
}

/// The CAR file `bytes` hold. Each block's CID is checked against its
/// bytes; a block whose CID is not one atproto blesses is refused.
pub fn read(bytes: &[u8]) -> Result<Car, CarError> {
    let mut r = Reader { bytes, at: 0 };
    let len = r.varint()?;
    let header = drisl::decode(r.take(len)?).or_else(|e| err(format!("the header: {e}")))?;
    let Value::Map(header) = header else {
        return err("the header is not a map");
    };
    let (Some(Value::Integer(1)), Some(Value::Array(roots))) =
        (header.get("version"), header.get("roots"))
    else {
        return err("the header is not version 1 with roots");
    };
    let roots = roots
        .iter()
        .map(|r| match r {
            Value::Link(c) => Ok(*c),
            _ => err("a root is not a link"),
        })
        .collect::<Result<_, _>>()?;
    let mut car = Car {
        roots,
        blocks: BTreeMap::new(),
    };
    while r.at < bytes.len() {
        let len = r.varint()?;
        let block = r.take(len)?;
        // A blessed CID is 36 bytes: version, codec, hash and its length.
        let Some((cid, data)) = block.split_at_checked(36) else {
            return err("a block too short for its CID");
        };
        let cid = Cid::from_bytes(cid).or_else(|e| err(format!("a block's CID: {e}")))?;
        // Cid::from_bytes allows only these two codecs.
        let hashed = if cid.codec() == DAG_CBOR {
            Cid::of_cbor(data)
        } else {
            Cid::of_raw(data)
        };
        if hashed != cid {
            return err(format!("block {cid} does not hash to its CID"));
        }
        car.blocks.insert(cid, data.to_vec());
    }
    Ok(car)
}
