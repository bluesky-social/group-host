//! The identifiers' syntax, against atproto's interop test vectors
//! (tests/interop/syntax, from github.com/bluesky-social/atproto-interop-tests).

use atproto::syntax::{is_did, is_handle, is_nsid, is_record_key, is_tid};

/// The lines of a vector file, less comments and blank lines.
fn lines(name: &str) -> Vec<String> {
    let path = format!("{}/tests/interop/syntax/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

fn check(kind: &str, is: fn(&str) -> bool) {
    for valid in lines(&format!("{kind}_syntax_valid.txt")) {
        assert!(is(&valid), "{kind} should be valid: {valid:?}");
    }
    for invalid in lines(&format!("{kind}_syntax_invalid.txt")) {
        assert!(!is(&invalid), "{kind} should be invalid: {invalid:?}");
    }
}

#[test]
fn dids() {
    check("did", is_did);
    assert!(!is_did("did:"));
    assert!(!is_did(&format!("did:web:{}", "a".repeat(2048))));
}

#[test]
fn handles() {
    check("handle", is_handle);
}

#[test]
fn nsids() {
    check("nsid", is_nsid);
    assert!(!is_nsid(""));
}

#[test]
fn record_keys() {
    check("recordkey", is_record_key);
}

#[test]
fn tids() {
    check("tid", is_tid);
}
