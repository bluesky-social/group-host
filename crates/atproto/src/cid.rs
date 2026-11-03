//! CIDs, as atproto blesses them: version 1, SHA-256, and either DAG-CBOR
//! (a record, a commit, a tree node) or raw (a blob).

use crate::base32;
use sha2::{Digest, Sha256};
use std::fmt;

/// The codec of a record, a commit or an MST node.
pub const DAG_CBOR: u8 = 0x71;
/// The codec of a blob.
pub const RAW: u8 = 0x55;
const SHA256: u8 = 0x12;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cid {
    codec: u8,
    digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CidError {
    /// Not multibase `b`, or not base32 within it.
    Encoding,
    /// Not version 1, SHA-256, DAG-CBOR or raw.
    Unblessed,
}

impl fmt::Display for CidError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            CidError::Encoding => "a CID is base32, starting with b",
            CidError::Unblessed => "a CID is version 1 and SHA-256, of DAG-CBOR or raw",
        })
    }
}

impl std::error::Error for CidError {}

impl Cid {
    /// The CID of DRISL bytes: a record's, a commit's, a tree node's.
    pub fn of_cbor(bytes: &[u8]) -> Cid {
        Cid::of(DAG_CBOR, bytes)
    }

    /// The CID of a blob's bytes.
    pub fn of_raw(bytes: &[u8]) -> Cid {
        Cid::of(RAW, bytes)
    }

    fn of(codec: u8, bytes: &[u8]) -> Cid {
        Cid {
            codec,
            digest: Sha256::digest(bytes).into(),
        }
    }

    pub fn codec(&self) -> u8 {
        self.codec
    }

    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// The CID as binary, as a CAR file and a DAG-CBOR link hold it.
    pub fn to_bytes(&self) -> Vec<u8> {
        [&[1, self.codec, SHA256, 32][..], &self.digest].concat()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Cid, CidError> {
        match bytes {
            [1, codec @ (DAG_CBOR | RAW), SHA256, 32, digest @ ..] if digest.len() == 32 => {
                Ok(Cid {
                    codec: *codec,
                    digest: digest.try_into().expect("32 bytes"),
                })
            }
            _ => Err(CidError::Unblessed),
        }
    }

    /// A CID as text: multibase `b`, base32 lower case.
    pub fn parse(text: &str) -> Result<Cid, CidError> {
        let bytes = text
            .strip_prefix('b')
            .and_then(|b32| base32::decode(base32::LOWER, b32))
            .ok_or(CidError::Encoding)?;
        Cid::from_bytes(&bytes)
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "b{}", base32::encode(base32::LOWER, &self.to_bytes()))
    }
}

impl fmt::Debug for Cid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Cid({self})")
    }
}
