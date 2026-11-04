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
    pub fn to_meta(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("$type".into(), Value::String(COMMIT_MAC.into())),
            ("ver".into(), Value::Integer(COMMIT_VERSION)),
            ("repo".into(), Value::String(self.repo.clone())),
            ("space".into(), Value::String(self.space.clone())),
            ("rev".into(), Value::String(self.rev.clone())),
            ("data".into(), Value::Bytes(self.data.to_vec())),
        ])
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

/// The longest a STAR archive's metadata, or a key in it, may be.
const STAR_FRAME_MAX: usize = 64 * 1024 * 1024;
/// The longest a varint in a STAR archive may be, in bytes.
const STAR_VARINT_MAX: usize = 6;
/// The longest a record's DRISL may be, in a repo's archive.
pub const RECORD_MAX: usize = 1024 * 1024;

fn varint(mut n: usize, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn frame(bytes: &[u8], out: &mut Vec<u8>) {
    varint(bytes.len(), out);
    out.extend(bytes);
}

/// A STAR archive (STAR-lite v1): `meta`, then each pair's key and value,
/// as they come. What the keys and values mean is the profile's that
/// `meta`'s `$type` names.
pub fn write_star<'a>(
    meta: &BTreeMap<String, Value>,
    pairs: impl IntoIterator<Item = (&'a [u8], &'a [u8])>,
) -> Vec<u8> {
    let mut out = STAR_MAGIC.to_vec();
    frame(&drisl::encode(&Value::Map(meta.clone())), &mut out);
    for (key, value) in pairs {
        frame(key, &mut out);
        frame(value, &mut out);
    }
    out
}

/// A STAR archive's parts: its metadata, a DRISL map with a `$type`, and
/// its pairs in the order they came.
#[derive(Debug, PartialEq)]
pub struct Star<'a> {
    pub meta: BTreeMap<String, Value>,
    pub pairs: Vec<(&'a [u8], &'a [u8])>,
}

/// A varint at the front of `bytes`, in its shortest form, and what
/// follows it; none if `bytes` is empty.
fn read_varint(bytes: &[u8]) -> Result<Option<(usize, &[u8])>, SpaceError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut n = 0usize;
    for (i, &b) in bytes.iter().enumerate().take(STAR_VARINT_MAX) {
        n += usize::from(b & 0x7f) << (7 * i);
        if b < 0x80 {
            // A last byte of zero only pads the number out.
            if b == 0 && i > 0 {
                return err("a STAR length not in its shortest form");
            }
            return Ok(Some((n, &bytes[i + 1..])));
        }
    }
    let long = bytes.len() >= STAR_VARINT_MAX;
    err(if long {
        "a STAR length too long"
    } else {
        "a STAR archive cut short"
    })
}

/// A frame, and the bytes after it.
type Framed<'a> = (&'a [u8], &'a [u8]);

/// A frame at the front of `bytes`, at most `max` long, and what follows
/// it; none if `bytes` is empty.
fn read_frame(bytes: &[u8], max: usize) -> Result<Option<Framed<'_>>, SpaceError> {
    let Some((len, rest)) = read_varint(bytes)? else {
        return Ok(None);
    };
    if len > max {
        return err(format!("a STAR frame of {len} bytes, past {max}"));
    }
    if rest.len() < len {
        return err("a STAR archive cut short");
    }
    Ok(Some(rest.split_at(len)))
}

/// A STAR archive read, each value at most `max_value` long. Its metadata
/// is checked as DRISL's one form; its pairs are left to its profile.
pub fn read_star_parts(bytes: &[u8], max_value: usize) -> Result<Star<'_>, SpaceError> {
    let Some(rest) = bytes.strip_prefix(&STAR_MAGIC[..]) else {
        return err("not a STAR archive");
    };
    let Some((meta, mut rest)) = read_frame(rest, STAR_FRAME_MAX)? else {
        return err("a STAR archive with no metadata");
    };
    let Ok(Value::Map(meta)) = drisl::decode(meta) else {
        return err("STAR metadata that is not a DRISL map");
    };
    if !matches!(meta.get("$type"), Some(Value::String(_))) {
        return err("STAR metadata with no $type");
    }
    let mut pairs = Vec::new();
    // The end comes only between pairs.
    while let Some((key, after)) = read_frame(rest, STAR_FRAME_MAX)? {
        let value = read_frame(after, max_value)?;
        let (value, after) = value.ok_or(SpaceError("a STAR key with no value".into()))?;
        pairs.push((key, value));
        rest = after;
    }
    Ok(Star { meta, pairs })
}

/// A key of a STAR archive whose keys are text.
fn key_text(key: &[u8]) -> Result<&str, SpaceError> {
    std::str::from_utf8(key).map_err(|_| SpaceError("a STAR key that is not text".into()))
}

/// `meta`'s text at `key`.
fn text(meta: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match meta.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// A repo as a STAR archive: its commit, then each record's path and DRISL,
/// in byte order of the path.
pub fn star(commit: &Commit, records: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    write_star(
        &commit.to_meta(),
        records.iter().map(|(p, v)| (p.as_bytes(), v.as_slice())),
    )
}

/// A repo's STAR archive read: its commit, and each record's DRISL by
/// path. The records are not checked against the commit here: that is
/// the reader's, as their CIDs are its set hash's elements.
pub fn read_star(bytes: &[u8]) -> Result<(Commit, BTreeMap<String, Vec<u8>>), SpaceError> {
    let Star { meta, pairs } = read_star_parts(bytes, RECORD_MAX)?;
    let data = match meta.get("data") {
        Some(Value::Bytes(b)) => <[u8; 32]>::try_from(b.as_slice()).ok(),
        _ => None,
    };
    let ours = text(&meta, "$type").as_deref() == Some(COMMIT_MAC)
        && meta.get("ver") == Some(&Value::Integer(COMMIT_VERSION));
    let (Some(repo), Some(space), Some(rev), Some(data), true) = (
        text(&meta, "repo"),
        text(&meta, "space"),
        text(&meta, "rev"),
        data,
        ours,
    ) else {
        return err("not a repo's STAR archive");
    };
    let mut records = BTreeMap::new();
    let mut last: Option<&[u8]> = None;
    for (key, value) in pairs {
        let path = key_text(key)?;
        if last.is_some_and(|l| l >= key) {
            return err("a STAR archive's records out of order");
        }
        last = Some(key);
        records.insert(path.to_string(), value.to_vec());
    }
    let commit = Commit {
        repo,
        space,
        rev,
        data,
    };
    Ok((commit, records))
}

/// A space meta archive's `$type`: what a space's authority keeps, moving
/// with it from one host to another.
pub const SPACE_META: &str = "atproto-space-meta";
/// The version of space meta archive this reads.
pub const SPACE_META_VERSION: i64 = 1;
const WRITER_PREFIX: &str = "writer/";

/// A repo in a space's writer set, as its authority keeps it: the repo's
/// revision and digest it last heard of, and the space's revision then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceWriter {
    pub did: String,
    pub rev: String,
    pub data: Vec<u8>,
    pub space_rev: String,
}

/// A space as its authority's host keeps it (`exportSpaceMeta`): its
/// policy, whatever state the policy keeps beyond itself (a memberspace's
/// members), by key, and its writer set.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceMeta {
    pub space: String,
    pub policy: BTreeMap<String, Value>,
    pub policy_state: Vec<(String, BTreeMap<String, Value>)>,
    pub writers: Vec<SpaceWriter>,
}

/// Which of a space meta archive's sections, in their order, `key` is in.
fn section(key: &str) -> u8 {
    match key {
        "policy" => 0,
        _ if key.starts_with(WRITER_PREFIX) => 2,
        _ => 1,
    }
}

/// A writer from its key and value, if they are one.
fn writer(key: &str, value: &BTreeMap<String, Value>) -> Option<SpaceWriter> {
    let did = key
        .strip_prefix(WRITER_PREFIX)
        .filter(|d| syntax::is_did(d))?;
    let tid = |k: &str| text(value, k).filter(|t| syntax::is_tid(t));
    let Some(Value::Bytes(data)) = value.get("data") else {
        return None;
    };
    Some(SpaceWriter {
        did: did.into(),
        rev: tid("rev")?,
        data: data.clone(),
        space_rev: tid("spaceRev")?,
    })
}

/// A space meta archive read: its sections each whole, in order, each key
/// once and in byte order, and every value a map.
pub fn read_space_meta(bytes: &[u8]) -> Result<SpaceMeta, SpaceError> {
    let Star { meta, pairs } = read_star_parts(bytes, usize::MAX)?;
    let space = text(&meta, "space").filter(|s| SpaceRef::parse(s).is_ok());
    let ours = text(&meta, "$type").as_deref() == Some(SPACE_META)
        && meta.get("ver") == Some(&Value::Integer(SPACE_META_VERSION));
    let (Some(space), true) = (space, ours) else {
        return err("not a space meta archive");
    };
    let mut policy = None;
    let mut policy_state = Vec::new();
    let mut writers = Vec::new();
    let mut at: Option<(u8, &[u8])> = None;
    for (key, value) in pairs {
        let text = key_text(key)?;
        let part = section(text);
        if at.is_some_and(|(s, last)| part < s || (part == s && key <= last)) {
            return err(format!("{text} out of order, or twice"));
        }
        at = Some((part, key));
        let Ok(Value::Map(value)) = drisl::decode(value) else {
            return err(format!("{text} is not a DRISL map"));
        };
        match part {
            0 => policy = Some(value),
            1 => policy_state.push((text.to_string(), value)),
            _ => {
                let found = writer(text, &value);
                writers.push(found.ok_or(SpaceError(format!("{text} is no writer")))?);
            }
        }
    }
    let policy = policy.ok_or(SpaceError("a space meta archive with no policy".into()))?;
    Ok(SpaceMeta {
        space,
        policy,
        policy_state,
        writers,
    })
}
