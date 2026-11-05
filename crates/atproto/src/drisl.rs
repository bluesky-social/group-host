//! DRISL, atproto's deterministic CBOR (DAG-CBOR): the bytes a record, a
//! commit and a tree node are hashed as. Written out in the one form it
//! allows, and read back refusing any other, so that what reads is what
//! was signed: the shortest lengths, map keys in order, no floats, no
//! tags but links.
//!
//! Records arrive as JSON, in which a link is `{"$link": cid}` and bytes
//! `{"$bytes": base64}`; [`Value::from_json`] and [`Value::to_json`] go
//! between the two.

use crate::cid::Cid;
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use std::collections::BTreeMap;
use std::fmt;

/// What a record can hold.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    String(String),
    Bytes(Vec<u8>),
    Link(Cid),
    Array(Vec<Value>),
    /// Keys in DRISL's order is the encoding's business; a map here is a map.
    Map(BTreeMap<String, Value>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrislError(pub String);

impl fmt::Display for DrislError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DrislError {}

fn err<T>(what: impl Into<String>) -> Result<T, DrislError> {
    Err(DrislError(what.into()))
}

/// How deep values may nest, so that bytes from anyone cannot run the
/// reader out of stack.
pub const MAX_DEPTH: usize = 128;

/// `value` in DRISL.
pub fn encode(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write(value, &mut out);
    out
}

/// The CID of `value`, as DRISL.
pub fn cid(value: &Value) -> Cid {
    Cid::of_cbor(&encode(value))
}

fn head(major: u8, n: u64, out: &mut Vec<u8>) {
    let m = major << 5;
    match n {
        0..=23 => out.push(m | n as u8),
        24..=0xff => out.extend([m | 24, n as u8]),
        0x100..=0xffff => {
            out.push(m | 25);
            out.extend((n as u16).to_be_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(m | 26);
            out.extend((n as u32).to_be_bytes());
        }
        _ => {
            out.push(m | 27);
            out.extend(n.to_be_bytes());
        }
    }
}

/// DRISL's order of map keys: shorter first, then bytewise.
fn key_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn write(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Null => out.push(0xf6),
        Value::Bool(b) => out.push(if *b { 0xf5 } else { 0xf4 }),
        Value::Integer(i) if *i >= 0 => head(0, *i as u64, out),
        Value::Integer(i) => head(1, (-1 - *i) as u64, out),
        Value::String(s) => {
            head(3, s.len() as u64, out);
            out.extend(s.as_bytes());
        }
        Value::Bytes(b) => {
            head(2, b.len() as u64, out);
            out.extend(b);
        }
        Value::Link(cid) => {
            head(6, 42, out);
            let bytes = cid.to_bytes();
            head(2, bytes.len() as u64 + 1, out);
            out.push(0);
            out.extend(bytes);
        }
        Value::Array(items) => {
            head(4, items.len() as u64, out);
            items.iter().for_each(|v| write(v, out));
        }
        Value::Map(map) => {
            head(5, map.len() as u64, out);
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| key_order(a, b));
            for k in keys {
                head(3, k.len() as u64, out);
                out.extend(k.as_bytes());
                write(&map[k], out);
            }
        }
    }
}

/// The value `bytes` hold, all of them, refusing any form but DRISL's one.
pub fn decode(bytes: &[u8]) -> Result<Value, DrislError> {
    let (value, read) = decode_first(bytes)?;
    if read != bytes.len() {
        return err("bytes after the value");
    }
    Ok(value)
}

/// The first value in `bytes`, and how many bytes it took: a firehose
/// frame is a header and a body, one after the other.
pub fn decode_first(bytes: &[u8]) -> Result<(Value, usize), DrislError> {
    let mut reader = Reader { bytes, at: 0 };
    let value = reader.value(0)?;
    Ok((value, reader.at))
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: u64) -> Result<&[u8], DrislError> {
        let left = (self.bytes.len() - self.at) as u64;
        if n > left {
            return err("the bytes end in the middle of a value");
        }
        let start = self.at;
        self.at += n as usize;
        Ok(&self.bytes[start..self.at])
    }

    /// A head's major type and the number it carries, refusing a number
    /// written longer than it needs, and lengths left open.
    fn head(&mut self) -> Result<(u8, u8, u64), DrislError> {
        let first = self.take(1)?[0];
        let (major, info) = (first >> 5, first & 31);
        let n = match info {
            0..=23 => u64::from(info),
            24 => u64::from(self.take(1)?[0]),
            25 => u64::from(u16::from_be_bytes(self.take(2)?.try_into().expect("2"))),
            26 => u64::from(u32::from_be_bytes(self.take(4)?.try_into().expect("4"))),
            27 => u64::from_be_bytes(self.take(8)?.try_into().expect("8")),
            _ => return err("an indefinite or reserved length"),
        };
        let shortest = match info {
            24 => n >= 24,
            25 => n > 0xff,
            26 => n > 0xffff,
            27 => n > 0xffff_ffff,
            _ => true,
        };
        if !shortest && major != 7 {
            return err("a number written longer than it needs");
        }
        Ok((major, info, n))
    }

    fn text(&mut self, n: u64) -> Result<String, DrislError> {
        let bytes = self.take(n)?.to_vec();
        String::from_utf8(bytes).or_else(|_| err("text that is not UTF-8"))
    }

    fn value(&mut self, depth: usize) -> Result<Value, DrislError> {
        if depth > MAX_DEPTH {
            return err("values nested too deep");
        }
        let (major, info, n) = self.head()?;
        Ok(match major {
            0 => Value::Integer(i64::try_from(n).or_else(|_| err("an integer past 64 bits"))?),
            1 => Value::Integer(
                i64::try_from(n)
                    .map(|n| -1 - n)
                    .or_else(|_| err("an integer past 64 bits"))?,
            ),
            2 => Value::Bytes(self.take(n)?.to_vec()),
            3 => Value::String(self.text(n)?),
            4 => {
                let mut items = Vec::new();
                for _ in 0..n {
                    items.push(self.value(depth + 1)?);
                }
                Value::Array(items)
            }
            5 => {
                let mut map = BTreeMap::new();
                let mut last: Option<String> = None;
                for _ in 0..n {
                    let (major, _, len) = self.head()?;
                    if major != 3 {
                        return err("a map key that is not text");
                    }
                    let key = self.text(len)?;
                    if let Some(last) = &last
                        && key_order(last, &key) != std::cmp::Ordering::Less
                    {
                        return err("map keys out of order, or repeated");
                    }
                    let v = self.value(depth + 1)?;
                    map.insert(key.clone(), v);
                    last = Some(key);
                }
                Value::Map(map)
            }
            6 if n == 42 => {
                let (major, _, len) = self.head()?;
                let bytes = self.take(len)?;
                match (major, bytes) {
                    (2, [0, cid @ ..]) => {
                        Value::Link(Cid::from_bytes(cid).or_else(|e| err(format!("a link: {e}")))?)
                    }
                    _ => return err("a link that is not bytes starting with 0"),
                }
            }
            6 => return err(format!("tag {n}: only links (42) are allowed")),
            _ => match info {
                20 => Value::Bool(false),
                21 => Value::Bool(true),
                22 => Value::Null,
                25..=27 => return err("a float"),
                _ => return err(format!("simple value {info}")),
            },
        })
    }
}

impl Value {
    /// The map this is, if it is one.
    pub fn into_map(self) -> Option<BTreeMap<String, Value>> {
        match self {
            Value::Map(map) => Some(map),
            _ => None,
        }
    }

    /// The bytes this is, if it is bytes.
    pub fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Value::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// The value a record's JSON holds, as atproto writes it: an object
    /// of `$link` alone a link, of `$bytes` alone bytes, a number with a
    /// fraction refused.
    pub fn from_json(json: &serde_json::Value) -> Result<Value, DrislError> {
        use serde_json::Value as J;
        Ok(match json {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => match n.as_i64() {
                Some(i) => Value::Integer(i),
                // JSON cannot tell 123.0 from 123, so neither does a record.
                None => match n.as_f64() {
                    Some(f) if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 => {
                        Value::Integer(f as i64)
                    }
                    _ => return err(format!("{n} is not an integer")),
                },
            },
            J::String(s) => Value::String(s.clone()),
            J::Array(items) => Value::Array(
                items
                    .iter()
                    .map(Value::from_json)
                    .collect::<Result<_, _>>()?,
            ),
            J::Object(map) => {
                if let Some(link) = map.get("$link") {
                    let (Some(text), 1) = (link.as_str(), map.len()) else {
                        return err("a $link is a CID's text, alone in its object");
                    };
                    return Cid::parse(text)
                        .map(Value::Link)
                        .or_else(|e| err(format!("$link: {e}")));
                }
                if let Some(bytes) = map.get("$bytes") {
                    let (Some(text), 1) = (bytes.as_str(), map.len()) else {
                        return err("$bytes is base64 text, alone in its object");
                    };
                    let text = text.trim_end_matches('=');
                    return STANDARD_NO_PAD
                        .decode(text)
                        .map(Value::Bytes)
                        .or_else(|_| err("$bytes is not base64"));
                }
                Value::Map(
                    map.iter()
                        .map(|(k, v)| Ok((k.clone(), Value::from_json(v)?)))
                        .collect::<Result<_, DrislError>>()?,
                )
            }
        })
    }

    /// The value as a record's JSON.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::{Value as J, json};
        match self {
            Value::Null => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Integer(i) => J::from(*i),
            Value::String(s) => J::String(s.clone()),
            Value::Bytes(b) => json!({ "$bytes": STANDARD_NO_PAD.encode(b) }),
            Value::Link(cid) => json!({ "$link": cid.to_string() }),
            Value::Array(items) => J::Array(items.iter().map(Value::to_json).collect()),
            Value::Map(map) => {
                J::Object(map.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())
            }
        }
    }
}

/// Whether `json` is a record as atproto's data model has one: an object,
/// whose every `$type` is a name, and whose every blob says what it holds.
/// What fails is said.
pub fn check_record(json: &serde_json::Value) -> Result<(), DrislError> {
    if !json.is_object() {
        return err("a record is an object");
    }
    Value::from_json(json)?;
    check_types(json)
}

fn check_types(json: &serde_json::Value) -> Result<(), DrislError> {
    use serde_json::Value as J;
    match json {
        J::Array(items) => items.iter().try_for_each(check_types),
        J::Object(map) => {
            match map.get("$type") {
                None => {}
                Some(J::String(t)) if t == "blob" && !is_blob(map) => {
                    return err("a blob has a ref, a mimeType and a size");
                }
                Some(J::String(t)) if !t.is_empty() => {}
                Some(_) => return err("a $type is a name"),
            }
            map.values().try_for_each(check_types)
        }
        _ => Ok(()),
    }
}

/// Whether an object whose `$type` is `blob` says what a blob does.
fn is_blob(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    map.get("ref").and_then(|r| r.get("$link")).is_some()
        && map
            .get("mimeType")
            .is_some_and(serde_json::Value::is_string)
        && map.get("size").is_some_and(serde_json::Value::is_i64)
}
