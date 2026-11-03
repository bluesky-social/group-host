//! PLC operations: how a `did:plc` is made, and how its document changes.
//! Each operation names the keys that may sign the next (its rotation
//! keys), the key the account's repo is signed with, its handle and its
//! PDS, and is signed by one of the rotation keys of the operation before
//! it. The DID is a hash of the first.

use crate::base32;
use crate::cid::Cid;
use crate::drisl::{self, Value};
use crate::keys::{Keypair, PublicKey};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlcError(pub String);

impl fmt::Display for PlcError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PlcError {}

fn err<T>(what: impl Into<String>) -> Result<T, PlcError> {
    Err(PlcError(what.into()))
}

/// A service an account's document names, such as its PDS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// Its type, as `AtprotoPersonalDataServer`.
    pub kind: String,
    pub endpoint: String,
}

/// What a `did:plc` document says, as one operation sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    /// The `did:key`s that may sign the next operation, the first ranked
    /// highest.
    pub rotation_keys: Vec<String>,
    /// Keys by name, as `atproto`, the repo's signing key, each a `did:key`.
    pub verification_methods: BTreeMap<String, String>,
    /// As `at://<handle>`.
    pub also_known_as: Vec<String>,
    /// Services by name, as `atproto_pds`.
    pub services: BTreeMap<String, Service>,
}

impl Op {
    /// An account at `handle` whose repo is signed with `signing` and
    /// held at `pds`, its operations signed with `rotation_keys`.
    pub fn atproto(signing: &str, rotation_keys: &[String], handle: &str, pds: &str) -> Op {
        Op {
            rotation_keys: rotation_keys.to_vec(),
            verification_methods: BTreeMap::from([("atproto".into(), signing.into())]),
            also_known_as: vec![format!("at://{handle}")],
            services: BTreeMap::from([(
                "atproto_pds".into(),
                Service {
                    kind: "AtprotoPersonalDataServer".into(),
                    endpoint: pds.into(),
                },
            )]),
        }
    }

    /// The handle the document names, its first `at://`.
    pub fn handle(&self) -> Option<&str> {
        self.also_known_as
            .iter()
            .find_map(|a| a.strip_prefix("at://"))
    }

    /// This operation after `prev` (none for the first), signed with
    /// `key`, which must be one of the rotation keys before it.
    pub fn sign(self, prev: Option<Cid>, key: &Keypair) -> Signed {
        let mut signed = Signed::Update {
            op: self,
            prev,
            sig: Vec::new(),
        };
        let sig = key.sign(&drisl::encode(&signed.unsigned())).to_vec();
        if let Signed::Update { sig: s, .. } = &mut signed {
            *s = sig;
        }
        signed
    }
}

/// An operation as it is published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signed {
    Update {
        op: Op,
        prev: Option<Cid>,
        sig: Vec<u8>,
    },
    /// The end of the DID: nothing after it.
    Tombstone { prev: Cid, sig: Vec<u8> },
}

/// The tombstone after `prev`, signed with `key`.
pub fn tombstone(prev: Cid, key: &Keypair) -> Signed {
    let mut signed = Signed::Tombstone {
        prev,
        sig: Vec::new(),
    };
    let sig = key.sign(&drisl::encode(&signed.unsigned())).to_vec();
    if let Signed::Tombstone { sig: s, .. } = &mut signed {
        *s = sig;
    }
    signed
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().map(|s| Value::String(s.clone())).collect())
}

impl Signed {
    /// The operation without its signature: what is signed.
    fn unsigned(&self) -> Value {
        Value::Map(self.fields())
    }

    fn fields(&self) -> BTreeMap<String, Value> {
        let s = |t: &str| Value::String(t.into());
        // A CID is text in an operation, not a DRISL link: the directory
        // signs and hashes it so.
        let link = |c: &Option<Cid>| c.map_or(Value::Null, |c| Value::String(c.to_string()));
        match self {
            Signed::Update { op, prev, .. } => BTreeMap::from([
                ("type".into(), s("plc_operation")),
                ("rotationKeys".into(), strings(&op.rotation_keys)),
                (
                    "verificationMethods".into(),
                    Value::Map(
                        op.verification_methods
                            .iter()
                            .map(|(k, v)| (k.clone(), s(v)))
                            .collect(),
                    ),
                ),
                ("alsoKnownAs".into(), strings(&op.also_known_as)),
                (
                    "services".into(),
                    Value::Map(
                        op.services
                            .iter()
                            .map(|(k, v)| {
                                let service = BTreeMap::from([
                                    ("type".into(), s(&v.kind)),
                                    ("endpoint".into(), s(&v.endpoint)),
                                ]);
                                (k.clone(), Value::Map(service))
                            })
                            .collect(),
                    ),
                ),
                ("prev".into(), link(prev)),
            ]),
            Signed::Tombstone { prev, .. } => BTreeMap::from([
                ("type".into(), s("plc_tombstone")),
                ("prev".into(), link(&Some(*prev))),
            ]),
        }
    }

    fn sig(&self) -> &[u8] {
        match self {
            Signed::Update { sig, .. } | Signed::Tombstone { sig, .. } => sig,
        }
    }

    /// The operation with its signature, as DRISL.
    fn value(&self) -> Value {
        let mut map = self.fields();
        map.insert(
            "sig".into(),
            Value::String(URL_SAFE_NO_PAD.encode(self.sig())),
        );
        Value::Map(map)
    }

    /// The CID of the signed operation: what the next one's `prev` names.
    pub fn cid(&self) -> Cid {
        drisl::cid(&self.value())
    }

    /// The DID this operation makes, as the first of its DID's.
    pub fn did(&self) -> String {
        let hash = Sha256::digest(drisl::encode(&self.value()));
        let text = base32::encode(base32::LOWER, &hash);
        format!("did:plc:{}", &text[..24])
    }

    pub fn prev(&self) -> Option<Cid> {
        match self {
            Signed::Update { prev, .. } => *prev,
            Signed::Tombstone { prev, .. } => Some(*prev),
        }
    }

    /// Which of `rotation_keys` (`did:key`s) signed it, by its place there.
    pub fn signed_by(&self, rotation_keys: &[String]) -> Option<usize> {
        let unsigned = drisl::encode(&self.unsigned());
        rotation_keys.iter().position(|k| {
            PublicKey::from_did_key(k).is_ok_and(|key| key.verify(&unsigned, self.sig()))
        })
    }

    /// The operation as the PLC directory takes it.
    pub fn to_json(&self) -> serde_json::Value {
        self.value().to_json()
    }

    /// An operation as the PLC directory gives it.
    pub fn from_json(json: &serde_json::Value) -> Result<Signed, PlcError> {
        use serde_json::Value as J;
        let text = |k: &str| json.get(k).and_then(J::as_str);
        let Some(sig) = text("sig").and_then(|s| URL_SAFE_NO_PAD.decode(s).ok()) else {
            return err("an operation's sig is base64url");
        };
        let prev = match json.get("prev") {
            Some(J::String(p)) => Some(Cid::parse(p).or_else(|e| err(format!("prev: {e}")))?),
            _ => None,
        };
        let list = |k: &str| -> Result<Vec<String>, PlcError> {
            let items = json.get(k).and_then(J::as_array);
            let items = items.map(|a| a.iter().map(|v| v.as_str().map(String::from)).collect());
            items
                .flatten()
                .ok_or_else(|| PlcError(format!("{k} is a list of text")))
        };
        let map = |k: &str| json.get(k).and_then(J::as_object);
        match text("type") {
            Some("plc_tombstone") => match prev {
                Some(prev) => Ok(Signed::Tombstone { prev, sig }),
                None => err("a tombstone names the operation it ends"),
            },
            Some("plc_operation") => {
                let methods = map("verificationMethods").map(|m| {
                    m.iter()
                        .map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                        .collect::<Option<BTreeMap<_, _>>>()
                });
                let services = map("services").map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            let kind = v.get("type")?.as_str()?.to_string();
                            let endpoint = v.get("endpoint")?.as_str()?.to_string();
                            Some((k.clone(), Service { kind, endpoint }))
                        })
                        .collect::<Option<BTreeMap<_, _>>>()
                });
                let (Some(Some(verification_methods)), Some(Some(services))) = (methods, services)
                else {
                    return err("an operation's keys and services are maps of text");
                };
                let op = Op {
                    rotation_keys: list("rotationKeys")?,
                    verification_methods,
                    also_known_as: list("alsoKnownAs")?,
                    services,
                };
                Ok(Signed::Update { op, prev, sig })
            }
            _ => err("not a plc_operation or plc_tombstone"),
        }
    }
}
