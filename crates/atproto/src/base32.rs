//! Base32 without padding, in the two alphabets atproto uses: RFC 4648's,
//! lower case, for CIDs (multibase `b`), and the sortable one TIDs are
//! written in.

/// RFC 4648's alphabet, lower case: a CID's multibase `b`.
pub const LOWER: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// The sortable alphabet a TID is written in, so that text order is time order.
pub const SORTABLE: &[u8; 32] = b"234567abcdefghijklmnopqrstuvwxyz";

pub fn encode(alphabet: &[u8; 32], bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let (mut buffer, mut bits) = (0u32, 0);
    for &b in bytes {
        buffer = (buffer << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(alphabet[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(alphabet[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// The bytes `text` holds, or `None` for a character outside `alphabet`
/// or bits left over that are not zero.
pub fn decode(alphabet: &[u8; 32], text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let (mut buffer, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let value = alphabet.iter().position(|&a| a == c)? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    (buffer & ((1 << bits) - 1) == 0).then_some(out)
}
