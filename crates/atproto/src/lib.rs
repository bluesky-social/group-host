//! The atproto a group host needs, written for it rather than taken from a
//! framework, as knotless writes its own: DRISL (DAG-CBOR) and CIDs, the
//! keys a repo and a PLC operation are signed with, the Merkle search tree
//! a repo's records hang from, CAR files, PLC operations, and the
//! service-auth tokens one server hands another.
//!
//! Each part is checked against atproto's interop test vectors, under
//! `tests/interop`. The parts still to come are in `docs/design/README.md`.

pub mod base32;
pub mod cid;
pub mod drisl;
pub mod keys;
pub mod syntax;
pub mod tid;

pub use cid::Cid;
pub use drisl::Value;
pub use keys::{Curve, Keypair, PublicKey};
