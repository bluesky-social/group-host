//! Permissioned spaces (the space alpha): what holds a repo in a space
//! together. A repo's records are summed into an LtHash, a set hash whose
//! digest is the repo's commit; a commit, or a record, is vouched for to
//! a syncing service with an HMAC under a secret agreed for the request
//! (a DHKEM over P-256); and a repo is sent whole as a STAR archive.
//!
//! As `@atproto/space` has them, checked against vectors made by running it
//! (`tests/reference/space-vectors.json`) and against knotless's client.

use crate::cid::Cid;
use crate::drisl::{self, Value};
use crate::keys::{Keypair, PublicKey};
use crate::syntax;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceError(pub String);

impl fmt::Display for SpaceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SpaceError {}

fn err<T>(what: impl Into<String>) -> Result<T, SpaceError> {
    Err(SpaceError(what.into()))
}

/// A space, as `at://<authority>/space/<type>/<skey>` names it: the DID
/// of its authority, its type (an NSID), and its key (a record key).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceRef {
    pub authority: String,
    pub kind: String,
    pub skey: String,
}

impl SpaceRef {
    /// The space `uri` names, written exactly as [`SpaceRef`]'s `Display`
    /// writes one.
    pub fn parse(uri: &str) -> Result<SpaceRef, SpaceError> {
        let parts: Vec<&str> = uri.strip_prefix("at://").unwrap_or("").split('/').collect();
        match parts[..] {
            [authority, "space", kind, skey]
                if syntax::is_did(authority)
                    && syntax::is_nsid(kind)
                    && syntax::is_record_key(skey) =>
            {
                Ok(SpaceRef {
                    authority: authority.into(),
                    kind: kind.into(),
                    skey: skey.into(),
                })
            }
            _ => err(format!("not a space: {uri}")),
        }
    }
}

impl fmt::Display for SpaceRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "at://{}/space/{}/{}",
            self.authority, self.kind, self.skey
        )
    }
}

/// The lanes of an LtHash, each a little-endian u16.
const LANES: usize = 1024;
/// An LtHash's whole state, in bytes.
pub const STATE_BYTES: usize = LANES * 2;

/// A homomorphic set hash: each element expands, by BLAKE3, to 1024 u16
/// lanes summed into the state mod 2^16, so it depends on the set alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LtHash([u16; LANES]);

impl Default for LtHash {
    fn default() -> LtHash {
        LtHash([0; LANES])
    }
}

impl LtHash {
    /// From a state kept with [`LtHash::state`].
    pub fn from_state(state: &[u8]) -> Result<LtHash, SpaceError> {
        if state.len() != STATE_BYTES {
            return err(format!("an LtHash state is {STATE_BYTES} bytes"));
        }
        let mut lanes = [0; LANES];
        for (lane, bytes) in lanes.iter_mut().zip(state.chunks(2)) {
            *lane = u16::from_le_bytes([bytes[0], bytes[1]]);
        }
        Ok(LtHash(lanes))
    }

    pub fn state(&self) -> Vec<u8> {
        self.0.iter().flat_map(|l| l.to_le_bytes()).collect()
    }

    pub fn add(&mut self, element: &str) {
        self.fold(element, u16::wrapping_add);
    }

    pub fn remove(&mut self, element: &str) {
        self.fold(element, u16::wrapping_sub);
    }

    fn fold(&mut self, element: &str, by: fn(u16, u16) -> u16) {
        let mut expanded = [0u8; STATE_BYTES];
        let mut hasher = blake3::Hasher::new();
        hasher.update(element.as_bytes());
        hasher.finalize_xof().fill(&mut expanded);
        for (lane, bytes) in self.0.iter_mut().zip(expanded.chunks(2)) {
            *lane = by(*lane, u16::from_le_bytes([bytes[0], bytes[1]]));
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.state()).into()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|l| *l == 0)
    }
}

/// A record's element of its repo's set hash: its path and its CID.
pub fn element(collection: &str, rkey: &str, cid: &Cid) -> String {
    format!("{collection}/{rkey}/{cid}")
}

/// The version of a space repo's commit.
pub const COMMIT_VERSION: i64 = 1;
/// What a commit's MAC, and a repo's STAR archive, are called.
pub const COMMIT_MAC: &str = "atproto-space-repo";
/// What a record's MAC is called.
pub const RECORD_MAC: &str = "atproto-space-record";
pub const RECORD_MAC_VERSION: i64 = 1;

/// A repo's commit in a space: which repo, in which space, at which
/// revision, and the digest of its records' set hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub repo: String,
    pub space: String,
    pub rev: String,
    pub data: [u8; 32],
}

/// The UTF-8 of `text`, after its length as a big-endian u16.
fn length_prefixed(text: &str) -> Vec<u8> {
    [&(text.len() as u16).to_be_bytes()[..], text.as_bytes()].concat()
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes a key of any length");
    parts.iter().for_each(|p| mac.update(p));
    mac.finalize().into_bytes().into()
}

impl Commit {
    /// The commit as its STAR archive's metadata holds it.
    pub fn to_value(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("$type".into(), Value::String(COMMIT_MAC.into())),
            ("ver".into(), Value::Integer(COMMIT_VERSION)),
            ("repo".into(), Value::String(self.repo.clone())),
            ("space".into(), Value::String(self.space.clone())),
            ("rev".into(), Value::String(self.rev.clone())),
            ("data".into(), Value::Bytes(self.data.to_vec())),
        ]))
    }

    /// The commit's MAC under `secret`.
    pub fn mac(&self, secret: &[u8]) -> [u8; 32] {
        let context = [
            format!("{COMMIT_MAC}-v{COMMIT_VERSION}").into_bytes(),
            length_prefixed(&self.space),
            length_prefixed(&self.repo),
            length_prefixed(&self.rev),
        ]
        .concat();
        hmac(secret, &[&context, &self.data])
    }
}

/// A record's MAC under `secret`: the record at `uri` is `cid`.
pub fn record_mac(secret: &[u8], uri: &str, cid: &Cid) -> [u8; 32] {
    let context = [
        format!("{RECORD_MAC}-v{RECORD_MAC_VERSION}").into_bytes(),
        length_prefixed(uri),
    ]
    .concat();
    hmac(secret, &[&context, &cid.to_bytes()])
}

/// The header a MAC is sent in (`atproto-space-mac`), a structured-field
/// dictionary, as the reference writes it.
pub fn mac_header(kind: &str, mac: &[u8]) -> String {
    format!("type=\"{kind}\", ver=1, mac=:{}:", STANDARD.encode(mac))
}

/// The MAC a `mac_header` of `kind` carries.
pub fn parse_mac_header(header: &str, kind: &str) -> Result<Vec<u8>, SpaceError> {
    let ours = header.contains(&format!("type=\"{kind}\"")) && header.contains("ver=1");
    let mac = header
        .split("mac=:")
        .nth(1)
        .and_then(|m| m.split(':').next());
    let mac = mac.filter(|_| ours).and_then(|m| STANDARD.decode(m).ok());
    mac.ok_or_else(|| SpaceError(format!("not a MAC header of {kind}")))
}

/// The header asking for a MAC (`atproto-space-want-mac`), with the key
/// share it is to be agreed from.
pub fn want_mac_header(kind: &str, key_share: &str) -> String {
    format!("type=\"{kind}\", ver=1, key_share=\"{key_share}\"")
}

/// The key share an `atproto-space-want-mac` header of `kind` sends.
pub fn parse_want_mac(header: &str, kind: &str) -> Result<String, SpaceError> {
    let mut fields = BTreeMap::new();
    for member in header.split(',') {
        let Some((key, value)) = member.trim().split_once('=') else {
            return err("a malformed MAC header");
        };
        fields.insert(key, value.trim_matches('"'));
    }
    if fields.get("type") != Some(&kind) || fields.get("ver") != Some(&"1") {
        return err(format!("a MAC header not of {kind}, version 1"));
    }
    match fields.get("key_share") {
        Some(share) if share.starts_with("did:key:") => Ok(share.to_string()),
        _ => err("a MAC header with no key share"),
    }
}

/// DHKEM(P-256, HKDF-SHA256)'s shared secret, RFC 9180 §4.1, from the
/// Diffie-Hellman `dh` of the syncer's key and the author's.
fn derive(dh: &[u8], syncer: &p256::PublicKey, author: &p256::PublicKey) -> [u8; 32] {
    let point = |k: &p256::PublicKey| k.to_encoded_point(false).as_bytes().to_vec();
    let suite = [&b"HPKE-v1KEM"[..], &0x0010u16.to_be_bytes()].concat();
    let ikm = [&suite[..], b"eae_prk", dh].concat();
    let info = [
        &32u16.to_be_bytes()[..],
        &suite,
        b"shared_secret",
        &point(syncer),
        &point(author),
    ]
    .concat();
    // HKDF with no salt, one block long.
    let prk = hmac(&[], &[&ikm]);
    hmac(&prk, &[&info, &[1]])
}

/// The P-256 key a `did:key` names.
fn p256_of(did_key: &str) -> Result<p256::PublicKey, SpaceError> {
    match PublicKey::from_did_key(did_key) {
        Ok(PublicKey::P256(k)) => Ok(p256::PublicKey::from(k)),
        _ => err("a KEM key is a P-256 did:key"),
    }
}

/// The secret a syncer's `key_share` agrees with the author's KEM key
/// `kem`, as the author's server works it out.
pub fn decapsulate(kem: &Keypair, key_share: &str) -> Result<[u8; 32], SpaceError> {
    let Keypair::P256(secret) = kem else {
        return err("a KEM key is P-256");
    };
    let syncer = p256_of(key_share)?;
    let secret = p256::SecretKey::from(secret.as_nonzero_scalar());
    let dh = p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), syncer.as_affine());
    Ok(derive(dh.raw_secret_bytes(), &syncer, &secret.public_key()))
}

/// A secret agreed with the author whose KEM key is `author`, as a syncer
/// makes one: the key share to send, and the secret.
pub fn encapsulate(author: &str) -> Result<(String, [u8; 32]), SpaceError> {
    let author = p256_of(author)?;
    let ephemeral = p256::ecdh::EphemeralSecret::random(&mut rand_core::OsRng);
    let mine = ephemeral.public_key();
    let dh = ephemeral.diffie_hellman(&author);
    let share = PublicKey::P256(p256::ecdsa::VerifyingKey::from(&mine)).did_key();
    Ok((share, derive(dh.raw_secret_bytes(), &mine, &author)))
}

/// The magic bytes a STAR archive starts with.
pub const STAR_MAGIC: [u8; 4] = [0x2a, 0x6c, 0x01, 0x00];

fn varint(mut n: usize, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

/// A repo as a STAR archive: its commit, then each record's path and DRISL,
/// in byte order of the path.
pub fn star(commit: &Commit, records: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = STAR_MAGIC.to_vec();
    let meta = drisl::encode(&commit.to_value());
    varint(meta.len(), &mut out);
    out.extend(meta);
    for (path, value) in records {
        varint(path.len(), &mut out);
        out.extend(path.as_bytes());
        varint(value.len(), &mut out);
        out.extend(value);
    }
    out
}

/// A varint at the front of `bytes`, and what follows it.
fn read_varint(bytes: &[u8]) -> Result<(usize, &[u8]), SpaceError> {
    let mut n = 0usize;
    for (i, b) in bytes.iter().enumerate().take(10) {
        n |= usize::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            return Ok((n, &bytes[i + 1..]));
        }
    }
    err("a STAR archive cut short")
}

/// A frame at the front of `bytes`: its length, then that many bytes.
fn read_frame(bytes: &[u8]) -> Result<(&[u8], &[u8]), SpaceError> {
    let (len, rest) = read_varint(bytes)?;
    if rest.len() < len {
        return err("a STAR archive cut short");
    }
    Ok(rest.split_at(len))
}

/// A repo's STAR archive read: its commit, and each record's DRISL by
/// path. The records are not checked against the commit here: that is
/// the reader's, as their CIDs are its set hash's elements.
pub fn read_star(bytes: &[u8]) -> Result<(Commit, BTreeMap<String, Vec<u8>>), SpaceError> {
    let Some(mut rest) = bytes.strip_prefix(&STAR_MAGIC[..]) else {
        return err("not a STAR archive");
    };
    let (meta, after) = read_frame(rest)?;
    rest = after;
    let meta = drisl::decode(meta);
    let Ok(Value::Map(meta)) = meta else {
        return err("STAR metadata that is not a DRISL map");
    };
    let field = |key: &str| meta.get(key);
    let text = |key: &str| match field(key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    };
    let data = match field("data") {
        Some(Value::Bytes(b)) if b.len() == 32 => {
            Some(<[u8; 32]>::try_from(b.as_slice()).expect("32 bytes"))
        }
        _ => None,
    };
    let ours = text("$type").as_deref() == Some(COMMIT_MAC)
        && field("ver") == Some(&Value::Integer(COMMIT_VERSION));
    let (Some(repo), Some(space), Some(rev), Some(data), true) =
        (text("repo"), text("space"), text("rev"), data, ours)
    else {
        return err("not a repo's STAR archive");
    };
    let mut records = BTreeMap::new();
    let mut last: Option<String> = None;
    while !rest.is_empty() {
        let (key, after) = read_frame(rest)?;
        let (value, after) = read_frame(after)?;
        rest = after;
        let path = String::from_utf8(key.to_vec())
            .map_err(|_| SpaceError("a STAR key that is not text".into()))?;
        if last.as_deref().is_some_and(|l| l >= path.as_str()) {
            return err("a STAR archive's records out of order");
        }
        last = Some(path.clone());
        records.insert(path, value.to_vec());
    }
    let commit = Commit {
        repo,
        space,
        rev,
        data,
    };
    Ok((commit, records))
}
