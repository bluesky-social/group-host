//! The atproto a group host needs, written for it rather than taken from a
//! framework, as knotless writes its own: DRISL (DAG-CBOR) and CIDs, the
//! keys a repo and a PLC operation are signed with, the Merkle search tree
//! a repo's records hang from, CAR files, PLC operations, and the
//! service-auth tokens one server hands another.
//!
//! Each part arrives with the pull that first needs it; see
//! `docs/design/README.md` for the order.
