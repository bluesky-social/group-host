//! The keys atproto signs with, secp256k1 (`ES256K`) and P-256 (`ES256`):
//! a group's repo and rotation keys, the host's own, and the keys in a DID
//! document, as multikeys and `did:key`s. Signatures are 64 bytes, `r`
//! then `s`, with `s` in the low half: atproto refuses any other.

use rand_core::OsRng;
use std::fmt;

const K256_CODEC: [u8; 2] = [0xe7, 0x01];
const P256_CODEC: [u8; 2] = [0x80, 0x24];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyError(pub &'static str);

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for KeyError {}

/// Which curve a key is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    K256,
    P256,
}

impl Curve {
    /// Its JWT algorithm.
    pub fn alg(self) -> &'static str {
        match self {
            Curve::K256 => "ES256K",
            Curve::P256 => "ES256",
        }
    }
}

/// A key that signs.
#[derive(Clone)]
pub enum Keypair {
    K256(k256::ecdsa::SigningKey),
    P256(p256::ecdsa::SigningKey),
}

impl fmt::Debug for Keypair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Keypair({})", self.public().did_key())
    }
}

impl Keypair {
    pub fn generate(curve: Curve) -> Keypair {
        match curve {
            Curve::K256 => Keypair::K256(k256::ecdsa::SigningKey::random(&mut OsRng)),
            Curve::P256 => Keypair::P256(p256::ecdsa::SigningKey::random(&mut OsRng)),
        }
    }

    /// The key whose secret is the 32 bytes `secret`.
    pub fn from_bytes(curve: Curve, secret: &[u8]) -> Result<Keypair, KeyError> {
        let bad = |_| KeyError("not a secret key on its curve");
        Ok(match curve {
            Curve::K256 => Keypair::K256(k256::ecdsa::SigningKey::from_slice(secret).map_err(bad)?),
            Curve::P256 => Keypair::P256(p256::ecdsa::SigningKey::from_slice(secret).map_err(bad)?),
        })
    }

    /// The 32 bytes of its secret, to keep.
    pub fn to_bytes(&self) -> [u8; 32] {
        match self {
            Keypair::K256(k) => k.to_bytes().into(),
            Keypair::P256(k) => k.to_bytes().into(),
        }
    }

    pub fn curve(&self) -> Curve {
        match self {
            Keypair::K256(_) => Curve::K256,
            Keypair::P256(_) => Curve::P256,
        }
    }

    pub fn public(&self) -> PublicKey {
        match self {
            Keypair::K256(k) => PublicKey::K256(*k.verifying_key()),
            Keypair::P256(k) => PublicKey::P256(*k.verifying_key()),
        }
    }

    /// `message` signed, hashed with SHA-256: `r` then `s`, low-S.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        use ecdsa::signature::Signer;
        match self {
            Keypair::K256(k) => {
                let sig: k256::ecdsa::Signature = k.sign(message);
                sig.normalize_s().unwrap_or(sig).to_bytes().into()
            }
            Keypair::P256(k) => {
                let sig: p256::ecdsa::Signature = k.sign(message);
                sig.normalize_s().unwrap_or(sig).to_bytes().into()
            }
        }
    }
}

/// A key that checks signatures: one a DID document or `did:key` names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicKey {
    K256(k256::ecdsa::VerifyingKey),
    P256(p256::ecdsa::VerifyingKey),
}

fn base58(text: &str) -> Result<Vec<u8>, KeyError> {
    let b58 = text
        .strip_prefix('z')
        .ok_or(KeyError("a multikey is base58btc, starting with z"))?;
    bs58::decode(b58)
        .into_vec()
        .map_err(|_| KeyError("bad base58 in a key"))
}

impl PublicKey {
    /// The key a multikey (multibase `z`, base58btc, with its multicodec) holds.
    pub fn from_multibase(text: &str) -> Result<PublicKey, KeyError> {
        let bytes = base58(text)?;
        let (codec, key) = bytes.split_at(2.min(bytes.len()));
        match <[u8; 2]>::try_from(codec) {
            Ok(K256_CODEC) => PublicKey::from_sec1(Curve::K256, key),
            Ok(P256_CODEC) => PublicKey::from_sec1(Curve::P256, key),
            _ => Err(KeyError("not a secp256k1 or P-256 multikey")),
        }
    }

    /// The key of a DID document's verification method: a `Multikey`, or
    /// one of the older types, whose multibase holds the bare key.
    pub fn from_method(kind: &str, multibase: &str) -> Result<PublicKey, KeyError> {
        match kind {
            "EcdsaSecp256k1VerificationKey2019" => {
                PublicKey::from_sec1(Curve::K256, &base58(multibase)?)
            }
            "EcdsaSecp256r1VerificationKey2019" => {
                PublicKey::from_sec1(Curve::P256, &base58(multibase)?)
            }
            _ => PublicKey::from_multibase(multibase),
        }
    }

    pub fn from_did_key(did: &str) -> Result<PublicKey, KeyError> {
        let multibase = did
            .strip_prefix("did:key:")
            .ok_or(KeyError("not a did:key"))?;
        PublicKey::from_multibase(multibase)
    }

    /// The key as SEC1 bytes, compressed or not.
    pub fn from_sec1(curve: Curve, bytes: &[u8]) -> Result<PublicKey, KeyError> {
        match curve {
            Curve::K256 => k256::ecdsa::VerifyingKey::from_sec1_bytes(bytes)
                .map(PublicKey::K256)
                .map_err(|_| KeyError("not a secp256k1 key")),
            Curve::P256 => p256::ecdsa::VerifyingKey::from_sec1_bytes(bytes)
                .map(PublicKey::P256)
                .map_err(|_| KeyError("not a P-256 key")),
        }
    }

    pub fn curve(&self) -> Curve {
        match self {
            PublicKey::K256(_) => Curve::K256,
            PublicKey::P256(_) => Curve::P256,
        }
    }

    /// The key as SEC1 bytes, compressed.
    pub fn to_sec1(&self) -> Vec<u8> {
        match self {
            PublicKey::K256(k) => k.to_encoded_point(true).as_bytes().to_vec(),
            PublicKey::P256(k) => k.to_encoded_point(true).as_bytes().to_vec(),
        }
    }

    /// The key as a multikey, as a DID document's `publicKeyMultibase`.
    pub fn to_multibase(&self) -> String {
        let codec = match self {
            PublicKey::K256(_) => K256_CODEC,
            PublicKey::P256(_) => P256_CODEC,
        };
        let bytes = [&codec[..], &self.to_sec1()].concat();
        format!("z{}", bs58::encode(bytes).into_string())
    }

    pub fn did_key(&self) -> String {
        format!("did:key:{}", self.to_multibase())
    }

    /// Whether `signature` is this key's of `message`, low-S or not: what an
    /// HTTP message signature on a space request is held to, as the space
    /// alpha takes either. Everywhere else, [`PublicKey::verify`].
    pub fn verify_either_s(&self, message: &[u8], signature: &[u8]) -> bool {
        use ecdsa::signature::Verifier;
        match self {
            PublicKey::K256(k) => k256::ecdsa::Signature::from_slice(signature)
                .is_ok_and(|s| k.verify(message, &s.normalize_s().unwrap_or(s)).is_ok()),
            PublicKey::P256(k) => p256::ecdsa::Signature::from_slice(signature)
                .is_ok_and(|s| k.verify(message, &s).is_ok()),
        }
    }

    /// Whether `signature` is this key's of `message`: 64 bytes, `r` then
    /// `s`, low-S. A DER signature, or a high-S one, is refused.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        use ecdsa::signature::Verifier;
        match self {
            PublicKey::K256(k) => k256::ecdsa::Signature::from_slice(signature)
                .is_ok_and(|s| s.normalize_s().is_none() && k.verify(message, &s).is_ok()),
            PublicKey::P256(k) => p256::ecdsa::Signature::from_slice(signature)
                .is_ok_and(|s| s.normalize_s().is_none() && k.verify(message, &s).is_ok()),
        }
    }
}
