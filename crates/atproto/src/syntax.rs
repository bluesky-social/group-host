//! What atproto's identifiers may look like, as its specs give their
//! syntax: DIDs, handles, NSIDs, record keys and TIDs. Each says only
//! whether a string is well formed, not whether it names anything.

/// A DID: `did:`, a lower-case method, and an identifier of letters,
/// digits and `._:%-` not ending in `:` or `%`, 2 KiB at most.
pub fn is_did(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("did:") else {
        return false;
    };
    let Some((method, id)) = rest.split_once(':') else {
        return false;
    };
    s.len() <= 2048
        && !method.is_empty()
        && method.bytes().all(|c| c.is_ascii_lowercase())
        && !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:%-".contains(&c))
        && !id.ends_with([':', '%'])
}

/// A handle: two or more labels of letters, digits and inner hyphens, each
/// 63 characters at most, the last not starting with a digit, 253 in all.
pub fn is_handle(s: &str) -> bool {
    let labels: Vec<&str> = s.split('.').collect();
    s.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| is_label(l))
        && !labels[labels.len() - 1].starts_with(|c: char| c.is_ascii_digit())
}

fn is_label(l: &str) -> bool {
    (1..=63).contains(&l.len())
        && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        && !l.starts_with('-')
        && !l.ends_with('-')
}

/// An NSID: a reversed domain of two or more labels, its first not
/// starting with a digit, then a name of letters and digits not starting
/// with one, 317 characters in all. The interop vectors hold a domain
/// longer than a hostname's 253 as valid, so this does too.
pub fn is_nsid(s: &str) -> bool {
    let Some((domain, name)) = s.rsplit_once('.') else {
        return false;
    };
    let domain: Vec<&str> = domain.split('.').collect();
    s.len() <= 317
        && domain.len() >= 2
        && domain.iter().all(|l| is_label(l))
        && !domain[0].starts_with(|c: char| c.is_ascii_digit())
        && (1..=63).contains(&name.len())
        && name.bytes().all(|c| c.is_ascii_alphanumeric())
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

/// A record key: 1 to 512 of letters, digits and `._:~-`, but not `.` or `..`.
pub fn is_record_key(s: &str) -> bool {
    (1..=512).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:~-".contains(&c))
        && s != "."
        && s != ".."
}

/// A TID: 13 characters of sortable base32, the first holding no top bit.
pub fn is_tid(s: &str) -> bool {
    let alphabet = crate::base32::SORTABLE;
    s.len() == 13
        && alphabet[..16].contains(&s.as_bytes()[0])
        && s.bytes().all(|c| alphabet.contains(&c))
}
