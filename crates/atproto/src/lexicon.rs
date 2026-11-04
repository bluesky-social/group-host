//! Values checked against lexicons: a record, an object, a method's
//! parameters or its input, against the def that describes it, as
//! atproto's reference implementation (`@atproto/lexicon`) checks one.
//!
//! [`Lexicons`] holds a set of lexicon documents and checks a value against
//! a def by its reference (`nsid` for its main, `nsid#name`), following
//! references across the documents. Every field type is checked: null,
//! boolean, integer, string, bytes, cid-link, blob, array, object, params,
//! ref, union, unknown; `required` and `nullable`; a string's length in
//! UTF-8 bytes and in graphemes, its format, `const` and `enum`; an
//! integer's range; an array's length; a blob's `accept` and `maxSize`.
//! `knownValues` are a hint, not a rule, and are not enforced. A token
//! names a value and is no field's type, so no value is one. The first
//! failure is answered, with the path to it: `input.roles[2].role`.
//!
//! Checked against the reference's verdicts on lexicon.js's cases and a
//! few more (`tests/reference/lexicon.json`), and the formats against
//! atproto's interop vectors. Where this departs from the reference:
//!
//! - An at-uri may name a permissioned space, `at://<did>/space/<type>/<skey>`,
//!   or a record in one, `…/<writer did>/<collection>/<rkey>`, which the
//!   reference does not know yet.
//! - A blob's `accept` and `maxSize` are enforced; the reference leaves
//!   them to whoever stores the blob.
//! - The formats are the spec's, as atproto's interop vectors hold them,
//!   where the reference is laxer for old records' sake: a `cid` string is
//!   never a version 0 CID, a `datetime` always has its seconds and a time
//!   zone, a `language` tag's primary subtag is lower case and repeats no
//!   variant or extension, and a `uri` is 8 KiB at most.

use crate::base32;
use crate::syntax::{is_did, is_handle, is_nsid, is_record_key, is_tid};
use base64::Engine;
use base64::engine::general_purpose::GeneralPurpose;
use base64::engine::{DecodePaddingMode, GeneralPurposeConfig};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;
use unicode_segmentation::UnicodeSegmentation;

/// Why a value is not what its def describes: where, and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexiconError {
    /// Where in the value: `input.roles[2].role`.
    pub path: String,
    /// What is wrong there: `must be a string`.
    pub message: String,
}

impl fmt::Display for LexiconError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.path, self.message)
    }
}

impl std::error::Error for LexiconError {}

type Checked = Result<(), LexiconError>;

fn fail(path: &str, message: impl Into<String>) -> Checked {
    Err(LexiconError {
        path: path.to_string(),
        message: message.into(),
    })
}

/// A set of lexicon documents, by their NSIDs.
#[derive(Debug, Clone, Default)]
pub struct Lexicons {
    docs: HashMap<String, Value>,
}

/// Base64 as `$bytes` holds it: padded or not.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_encode_padding(false)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

impl Lexicons {
    /// The documents `docs`, each by its `id`; one without is left out.
    pub fn new(docs: impl IntoIterator<Item = Value>) -> Lexicons {
        let docs = docs.into_iter().filter_map(|doc| {
            let id = doc["id"].as_str()?.to_string();
            Some((id, doc))
        });
        Lexicons {
            docs: docs.collect(),
        }
    }

    /// The def `reference` names, from within the document `base`: `#name`,
    /// `nsid#name`, or `nsid` for its main; with the NSID it is in.
    fn resolve<'a>(&'a self, base: &str, reference: &str) -> Option<(String, &'a Value)> {
        let (nsid, name) = match reference.split_once('#') {
            Some(("", name)) => (base, name),
            Some((nsid, name)) => (nsid, name),
            None => (reference, "main"),
        };
        let def = self.docs.get(nsid)?.get("defs")?.get(name)?;
        Some((nsid.to_string(), def))
    }

    /// The def `reference` names: `nsid` for its main, or `nsid#name`.
    pub fn def(&self, reference: &str) -> Option<&Value> {
        self.resolve("", reference).map(|(_, def)| def)
    }

    /// The def, of one of `types`, that `reference` names.
    fn def_of(&self, reference: &str, types: &[&str], path: &str) -> Result<&Value, LexiconError> {
        match self.def(reference) {
            Some(def) if types.contains(&def["type"].as_str().unwrap_or_default()) => Ok(def),
            Some(_) => Err(LexiconError {
                path: path.to_string(),
                message: format!("{reference} is not a {}", types.join(" or ")),
            }),
            None => Err(LexiconError {
                path: path.to_string(),
                message: format!("no lexicon {reference}"),
            }),
        }
    }

    /// Checks `value` against the record or object `reference` names.
    pub fn validate(&self, reference: &str, value: &Value) -> Checked {
        let def = self.def_of(reference, &["record", "object"], "record")?;
        let nsid = nsid_of(reference);
        special(value, "record")?;
        match def["type"].as_str() {
            Some("record") => self.object(nsid, &def["record"], value, "record"),
            _ => self.object(nsid, def, value, "object"),
        }
    }

    /// Checks `params` against the parameters of the method `nsid`.
    pub fn validate_params(&self, nsid: &str, params: &Map<String, Value>) -> Checked {
        let types = ["query", "procedure", "subscription"];
        let def = &self.def_of(nsid, &types, "params")?["parameters"];
        let Some(properties) = def["properties"].as_object() else {
            return Ok(());
        };
        for (key, prop) in properties {
            let path = format!("params.{key}");
            match params.get(key) {
                Some(v) => {
                    special(v, &path)?;
                    self.field(nsid, prop, v, &path)?;
                }
                None if listed(def, "required", key) && !defaulted(prop) => {
                    fail("params", format!("must have the property \"{key}\""))?;
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Checks `input` against the JSON input of the procedure `nsid`.
    pub fn validate_input(&self, nsid: &str, input: &Value) -> Checked {
        let def = self.def_of(nsid, &["procedure"], "input")?;
        let schema = &def["input"]["schema"];
        if schema.is_null() {
            return Ok(());
        }
        special(input, "input")?;
        self.one_of(nsid, schema, input, "input", true)
    }

    /// The parameters a query string's `pairs` give the method `nsid`,
    /// each read as its def's type: an integer or a boolean where one
    /// parses, all of a name's values for an array. What does not parse
    /// is left a string, for [`Lexicons::validate_params`] to refuse; a
    /// name the method does not take is left out.
    pub fn query_params(&self, nsid: &str, pairs: &[(String, String)]) -> Map<String, Value> {
        let properties = self.def(nsid).map(|d| &d["parameters"]["properties"]);
        let mut out = Map::new();
        let Some(properties) = properties.and_then(Value::as_object) else {
            return out;
        };
        for (key, prop) in properties {
            let given = pairs.iter().filter(|(k, _)| k == key);
            let values: Vec<Value> = given.map(|(_, v)| typed(&prop["items"], prop, v)).collect();
            if prop["type"] == "array" {
                out.insert(key.clone(), Value::Array(values));
            } else if let Some(first) = values.into_iter().next() {
                out.insert(key.clone(), first);
            }
        }
        out.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
        out
    }

    /// Checks `value` against `def`, written in the document `nsid`: a ref
    /// or a union followed to the def it names, which then cannot be one.
    /// `object`, for a method's input, takes it as an object whatever the
    /// def it names.
    fn one_of(&self, nsid: &str, def: &Value, value: &Value, path: &str, object: bool) -> Checked {
        let (nsid, concrete) = match def["type"].as_str() {
            Some("union") => {
                let Some(kind) = value.get("$type").and_then(Value::as_str) else {
                    return fail(
                        path,
                        "must be an object which includes the \"$type\" property",
                    );
                };
                let refs: Vec<String> = def["refs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|r| absolute(nsid, r))
                    .collect();
                let listed = !kind.starts_with('#') && refs.contains(&absolute("", kind));
                if !listed && (def["closed"] == true || kind.starts_with('#')) {
                    return fail(path, format!("$type must be one of {}", refs.join(", ")));
                }
                if !listed {
                    return Ok(());
                }
                self.named(nsid, kind, path)?
            }
            Some("ref") => self.named(nsid, def["ref"].as_str().unwrap_or_default(), path)?,
            _ => (nsid.to_string(), def),
        };
        if object {
            self.object(&nsid, concrete, value, path)
        } else {
            self.field(&nsid, concrete, value, path)
        }
    }

    /// The def `reference`, written in `nsid`, names.
    fn named(
        &self,
        nsid: &str,
        reference: &str,
        path: &str,
    ) -> Result<(String, &Value), LexiconError> {
        self.resolve(nsid, reference).ok_or_else(|| LexiconError {
            path: path.to_string(),
            message: format!("no lexicon {}", absolute(nsid, reference)),
        })
    }

    /// Checks `value` against the field type `def`, written in `nsid`.
    fn field(&self, nsid: &str, def: &Value, value: &Value, path: &str) -> Checked {
        match def["type"].as_str().unwrap_or_default() {
            "object" => self.object(nsid, def, value, path),
            "array" => self.array(nsid, def, value, path),
            "blob" => blob(def, value, path),
            "boolean" => boolean(def, value, path),
            "integer" => integer(def, value, path),
            "string" => string(def, value, path),
            "bytes" => bytes(def, value, path),
            "cid-link" => match link(value) {
                Some(Ok(_)) => Ok(()),
                _ => fail(path, "must be a CID"),
            },
            "unknown" if value.is_object() || value.is_array() => Ok(()),
            "unknown" => fail(path, "must be an object"),
            other => fail(
                path,
                format!("is checked against a {other}, which is no field's type"),
            ),
        }
    }

    fn object(&self, nsid: &str, def: &Value, value: &Value, path: &str) -> Checked {
        let Some(map) = value.as_object() else {
            return fail(path, "must be an object");
        };
        let Some(properties) = def["properties"].as_object() else {
            return Ok(());
        };
        for (key, prop) in properties {
            let at = format!("{path}.{key}");
            match map.get(key) {
                Some(Value::Null) if listed(def, "nullable", key) => {}
                Some(v) => self.one_of(nsid, prop, v, &at, false)?,
                None if listed(def, "required", key) && !defaulted(prop) => {
                    fail(path, format!("must have the property \"{key}\""))?;
                }
                None => {}
            }
        }
        Ok(())
    }

    fn array(&self, nsid: &str, def: &Value, value: &Value, path: &str) -> Checked {
        let Some(items) = value.as_array() else {
            return fail(path, "must be an array");
        };
        if let Some(max) = def["maxLength"]
            .as_u64()
            .filter(|m| items.len() as u64 > *m)
        {
            fail(path, format!("must not have more than {max} elements"))?;
        }
        if let Some(min) = def["minLength"]
            .as_u64()
            .filter(|m| (items.len() as u64) < *m)
        {
            fail(path, format!("must not have fewer than {min} elements"))?;
        }
        for (i, item) in items.iter().enumerate() {
            self.one_of(nsid, &def["items"], item, &format!("{path}[{i}]"), false)?;
        }
        Ok(())
    }
}

/// The NSID a reference is in.
fn nsid_of(reference: &str) -> &str {
    reference.split('#').next().unwrap_or_default()
}

/// `reference`, written in `nsid`, as `nsid#name`.
fn absolute(nsid: &str, reference: &str) -> String {
    match reference.split_once('#') {
        Some(("", name)) => format!("{nsid}#{name}"),
        Some(_) => reference.to_string(),
        None => format!("{reference}#main"),
    }
}

/// Whether `def`'s list `list` (`required`, `nullable`) holds `key`.
fn listed(def: &Value, list: &str, key: &str) -> bool {
    def[list]
        .as_array()
        .is_some_and(|l| l.iter().any(|k| k == key))
}

/// Whether a field left out takes its def's default instead.
fn defaulted(def: &Value) -> bool {
    match def["type"].as_str() {
        Some("string") => def["default"].is_string(),
        Some("integer") => def["default"].is_number(),
        Some("boolean") => def["default"].is_boolean(),
        _ => false,
    }
}

/// A query string's `value` as the type `def` (or, for an array, `items`)
/// names.
fn typed(items: &Value, def: &Value, value: &str) -> Value {
    let kind = if def["type"] == "array" {
        &items["type"]
    } else {
        &def["type"]
    };
    let parsed = match kind.as_str() {
        Some("integer") => value.parse::<i64>().ok().map(Value::from),
        Some("boolean") => value.parse::<bool>().ok().map(Value::from),
        _ => None,
    };
    parsed.unwrap_or_else(|| Value::from(value))
}

/// Refuses a link or bytes in `value` that is not one, wherever it is,
/// as reading JSON as atproto data does: `{"$link": cid}` holding no CID,
/// `{"$bytes": base64}` holding no base64.
fn special(value: &Value, path: &str) -> Checked {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(i, v)| special(v, &format!("{path}[{i}]"))),
        Value::Object(map) if map.len() == 1 && map.contains_key("$link") => match link(value) {
            Some(Ok(_)) => Ok(()),
            _ => fail(path, "must hold a CID in $link"),
        },
        Value::Object(map) if map.len() == 1 && map.contains_key("$bytes") => {
            match decoded(value) {
                Some(_) => Ok(()),
                None => fail(path, "must hold base64 in $bytes"),
            }
        }
        Value::Object(map) => map
            .iter()
            .try_for_each(|(k, v)| special(v, &format!("{path}.{k}"))),
        _ => Ok(()),
    }
}

/// The bytes `{"$bytes": base64}` holds, or `None` for what is not that.
fn decoded(value: &Value) -> Option<Vec<u8>> {
    let map = value.as_object().filter(|m| m.len() == 1)?;
    BASE64.decode(map.get("$bytes")?.as_str()?).ok()
}

/// What `{"$link": cid}` holds: `None` for what is not that, an error for
/// one holding no CID.
fn link(value: &Value) -> Option<Result<(), ()>> {
    let map = value.as_object().filter(|m| m.len() == 1)?;
    let cid = map.get("$link")?;
    Some(match cid.as_str() {
        Some(cid) if cid.len() <= 2048 && is_cid(cid) => Ok(()),
        _ => Err(()),
    })
}

/// Whether `text` is a CID, as the reference parses one: version 1 in
/// base32 (`b…`) or base58 (`z…`), or version 0 (`Qm…`).
fn is_cid(text: &str) -> bool {
    let bytes = match text.as_bytes().first() {
        Some(b'b') => base32::decode(base32::LOWER, &text[1..]),
        Some(b'z') => bs58::decode(&text[1..]).into_vec().ok(),
        Some(b'Q') => {
            let v0 = bs58::decode(text).into_vec().ok();
            return v0.is_some_and(|b| b.len() == 34 && b[..2] == [0x12, 0x20]);
        }
        _ => None,
    };
    bytes.is_some_and(|b| is_cid_v1(&b))
}

/// Whether `bytes` are a binary CID of version 1: the version, a codec,
/// and a multihash holding exactly the digest it says.
fn is_cid_v1(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    let mut next = || {
        let (n, read) = varint(rest)?;
        rest = &rest[read..];
        Some(n)
    };
    let (Some(1), Some(_codec), Some(_hash), Some(len)) = (next(), next(), next(), next()) else {
        return false;
    };
    rest.len() as u64 == len
}

/// An unsigned varint at the start of `bytes`, and how many bytes it took.
fn varint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut n = 0u64;
    for (i, b) in bytes.iter().take(9).enumerate() {
        n |= u64::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            return Some((n, i + 1));
        }
    }
    None
}

fn boolean(def: &Value, value: &Value, path: &str) -> Checked {
    let Some(b) = value.as_bool() else {
        return fail(path, "must be a boolean");
    };
    match def["const"].as_bool() {
        Some(c) if c != b => fail(path, format!("must be {c}")),
        _ => Ok(()),
    }
}

/// `value` as an integer, as JSON numbers are one: `2.0` is.
fn as_integer(value: &Value) -> Option<f64> {
    let n = value.as_f64()?;
    (n.fract() == 0.0).then_some(n)
}

fn integer(def: &Value, value: &Value, path: &str) -> Checked {
    let Some(n) = as_integer(value) else {
        return fail(path, "must be an integer");
    };
    if let Some(c) = def["const"].as_f64().filter(|c| *c != n) {
        fail(path, format!("must be {c}"))?;
    }
    if let Some(e) = def["enum"].as_array()
        && !e.iter().any(|x| x.as_f64() == Some(n))
    {
        fail(path, format!("must be one of {}", Value::from(e.clone())))?;
    }
    if let Some(max) = def["maximum"].as_f64().filter(|m| n > *m) {
        fail(path, format!("can not be greater than {max}"))?;
    }
    if let Some(min) = def["minimum"].as_f64().filter(|m| n < *m) {
        fail(path, format!("can not be less than {min}"))?;
    }
    Ok(())
}

fn string(def: &Value, value: &Value, path: &str) -> Checked {
    let Some(s) = value.as_str() else {
        return fail(path, "must be a string");
    };
    if let Some(c) = def["const"].as_str().filter(|c| c != &s) {
        fail(path, format!("must be {c}"))?;
    }
    if let Some(e) = def["enum"].as_array()
        && !e.iter().any(|x| x == s)
    {
        fail(path, format!("must be one of {}", Value::from(e.clone())))?;
    }
    let bytes = s.len() as u64;
    if let Some(max) = def["maxLength"].as_u64().filter(|m| bytes > *m) {
        fail(path, format!("must not be longer than {max} bytes"))?;
    }
    if let Some(min) = def["minLength"].as_u64().filter(|m| bytes < *m) {
        fail(path, format!("must not be shorter than {min} bytes"))?;
    }
    if def["maxGraphemes"].is_u64() || def["minGraphemes"].is_u64() {
        let graphemes = s.graphemes(true).count() as u64;
        if let Some(max) = def["maxGraphemes"].as_u64().filter(|m| graphemes > *m) {
            fail(path, format!("must not be longer than {max} graphemes"))?;
        }
        if let Some(min) = def["minGraphemes"].as_u64().filter(|m| graphemes < *m) {
            fail(path, format!("must not be shorter than {min} graphemes"))?;
        }
    }
    match def["format"].as_str() {
        Some(format) if !is_format(format, s) => fail(path, format!("must be a valid {format}")),
        _ => Ok(()),
    }
}

fn bytes(def: &Value, value: &Value, path: &str) -> Checked {
    let Some(b) = decoded(value) else {
        return fail(path, "must be a byte array");
    };
    let len = b.len() as u64;
    if let Some(max) = def["maxLength"].as_u64().filter(|m| len > *m) {
        fail(path, format!("must not be larger than {max} bytes"))?;
    }
    if let Some(min) = def["minLength"].as_u64().filter(|m| len < *m) {
        fail(path, format!("must not be smaller than {min} bytes"))?;
    }
    Ok(())
}

/// A blob's type and size, when `value` is one: `{$type: "blob", ref,
/// mimeType, size}`, or the older `{cid, mimeType}`, whose size is unknown.
fn blob_ref(value: &Value) -> Option<(&str, Option<f64>)> {
    let map = value.as_object()?;
    let mime = map.get("mimeType")?.as_str()?;
    let keys = |want: &[&str]| map.len() == want.len() && want.iter().all(|k| map.contains_key(*k));
    if keys(&["$type", "ref", "mimeType", "size"]) {
        let typed = map["$type"] == "blob" && link(&map["ref"]) == Some(Ok(()));
        let size = map["size"].as_f64().filter(|_| typed)?;
        return Some((mime, Some(size)));
    }
    let cid = map.get("cid")?.as_str()?;
    (keys(&["cid", "mimeType"]) && is_cid(cid)).then_some((mime, None))
}

fn blob(def: &Value, value: &Value, path: &str) -> Checked {
    let Some((mime, size)) = blob_ref(value) else {
        return fail(path, "should be a blob ref");
    };
    if let Some(max) = def["maxSize"].as_f64()
        && size.is_some_and(|s| s > max)
    {
        fail(path, format!("must not be larger than {max} bytes"))?;
    }
    let accept: Vec<&str> = def["accept"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let accepted = |pattern: &&str| match pattern.strip_suffix("/*") {
        Some(kind) => kind == "*" || mime.split('/').next() == Some(kind),
        None => *pattern == mime,
    };
    if !accept.is_empty() && !accept.iter().any(accepted) {
        fail(
            path,
            format!("must be one of {} types, not {mime}", accept.join(", ")),
        )?;
    }
    Ok(())
}

/// Whether `s` is well formed as the string format `format`; a format
/// this does not know takes any string.
pub fn is_format(format: &str, s: &str) -> bool {
    match format {
        "datetime" => is_datetime(s),
        "uri" => is_uri(s),
        "at-uri" => is_at_uri(s),
        "did" => is_did(s),
        "handle" => is_handle(s),
        "at-identifier" => is_at_identifier(s),
        "nsid" => is_nsid(s),
        "cid" => is_cid_string(s),
        "language" => is_language(s),
        "tid" => is_tid(s),
        "record-key" => is_record_key(s),
        _ => true,
    }
}

/// A DID, or a handle.
pub fn is_at_identifier(s: &str) -> bool {
    if s.starts_with("did:") {
        is_did(s)
    } else {
        is_handle(s)
    }
}

/// A CID as the `cid` format writes one: 8 to 256 letters, digits, `+`
/// and `=`, and never a version 0 CID's `Qmb`.
pub fn is_cid_string(s: &str) -> bool {
    (8..=256).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'=')
        && !s.starts_with("Qmb")
}

/// A URI: a scheme (a letter, then letters, digits, `+`, `-` and `.`), a
/// colon, and no whitespace, 8 KiB at most.
pub fn is_uri(s: &str) -> bool {
    let Some((scheme, rest)) = s.split_once(':') else {
        return false;
    };
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    s.len() <= 8192
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"+-.".contains(&c))
        && !rest.starts_with('/')
        && !rest.is_empty()
        && !rest.chars().any(char::is_whitespace)
}

/// An AT URI: an authority, and a collection and a record key; or a
/// permissioned space, `at://<did>/space/<type>/<skey>`, and a record in
/// one, `…/<writer>/<collection>/<rkey>`. A fragment is a JSON pointer.
pub fn is_at_uri(s: &str) -> bool {
    let allowed = |c: u8| c.is_ascii_alphanumeric() || b"._~:@!$&'()*+,;=%/\\[]#?-".contains(&c);
    let Some(rest) = s.strip_prefix("at://") else {
        return false;
    };
    if s.len() > 8192 || !s.bytes().all(allowed) || rest.contains('?') {
        return false;
    }
    let (path, fragment) = match rest.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (rest, None),
    };
    let parts: Vec<&str> = path.split('/').collect();
    let well_formed = match parts[..] {
        [authority, "space", kind, skey] => is_space(authority, kind, skey),
        [authority, "space", kind, skey, writer, collection, rkey] => {
            is_space(authority, kind, skey)
                && is_did(writer)
                && is_nsid(collection)
                && is_record_key(rkey)
        }
        [authority] => is_at_identifier(authority),
        [authority, collection] => is_at_identifier(authority) && is_nsid(collection),
        [authority, collection, rkey] => {
            is_at_identifier(authority) && is_nsid(collection) && is_record_key(rkey)
        }
        _ => false,
    };
    well_formed && fragment.is_none_or(is_pointer)
}

fn is_space(authority: &str, kind: &str, skey: &str) -> bool {
    is_did(authority) && is_nsid(kind) && is_record_key(skey)
}

/// A JSON pointer as an AT URI's fragment writes one: `/…`, of URI
/// characters, percent-encoding UTF-8.
fn is_pointer(s: &str) -> bool {
    let allowed = |c: u8| c.is_ascii_alphanumeric() || b"._~:@!$&')(*+,;=%[]/-".contains(&c);
    if !s.starts_with('/') || !s.bytes().all(allowed) {
        return false;
    }
    let mut out = Vec::new();
    let mut bytes = s.bytes();
    while let Some(c) = bytes.next() {
        if c != b'%' {
            out.push(c);
            continue;
        }
        let hex: String = bytes.by_ref().take(2).map(char::from).collect();
        match u8::from_str_radix(&hex, 16) {
            Ok(b) if hex.len() == 2 => out.push(b),
            _ => return false,
        }
    }
    String::from_utf8(out).is_ok()
}

/// The digits `s` is, all of them.
fn digits(s: &str) -> Option<i64> {
    s.bytes()
        .all(|c| c.is_ascii_digit())
        .then(|| s.parse().ok())?
}

/// A datetime as the spec writes one: `YYYY-MM-DDTHH:MM:SS`, a fraction
/// of a second if any, and `Z` or `±HH:MM` (not `-00:00`); 64 characters
/// at most, and in UTC within the years 0 to 9999. A day past its month's
/// end runs on into the next month, as the reference reads one.
pub fn is_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    if s.len() > 64 || s.len() < 20 || s.ends_with("-00:00") || !s.is_ascii() {
        return false;
    }
    let shape = b[4] == b'-' && b[7] == b'-' && b[10] == b'T' && b[13] == b':' && b[16] == b':';
    let number = |from: usize, to: usize| digits(&s[from..to]);
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    let rest = &s[19..];
    let zone = match rest.find(['Z', '+', '-']) {
        Some(at) => &rest[at..],
        None => return false,
    };
    let fraction = &rest[..rest.len() - zone.len()];
    let fraction_ok = fraction.is_empty()
        || fraction.len() > 1
            && fraction.starts_with('.')
            && fraction[1..].bytes().all(|c| c.is_ascii_digit());
    let offset = match zone.as_bytes() {
        [b'Z'] => Some(0),
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let hm = |a: &u8, b: &u8| digits(&format!("{}{}", *a as char, *b as char));
            match (hm(h1, h2), hm(m1, m2)) {
                (Some(h), Some(m)) if h < 24 && m < 60 => {
                    Some(if *sign == b'-' { -1 } else { 1 } * (h * 60 + m))
                }
                _ => None,
            }
        }
        _ => None,
    };
    let in_range = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 60;
    let Some(offset) = offset.filter(|_| shape && fraction_ok && in_range) else {
        return false;
    };
    let minutes = (days_from_civil(year, month) + day - 1) * 1440 + hour * 60 + minute - offset;
    let first = days_from_civil(0, 1) * 1440;
    let last = days_from_civil(10000, 1) * 1440;
    (first..last).contains(&minutes)
}

/// The days from 1970-01-01 to the first of `month` in `year`.
fn days_from_civil(year: i64, month: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The tags RFC 5646 keeps from before its grammar.
const GRANDFATHERED: [&str; 26] = [
    "en-GB-oed",
    "i-ami",
    "i-bnn",
    "i-default",
    "i-enochian",
    "i-hak",
    "i-klingon",
    "i-lux",
    "i-mingo",
    "i-navajo",
    "i-pwn",
    "i-tao",
    "i-tay",
    "i-tsu",
    "sgn-BE-FR",
    "sgn-BE-NL",
    "sgn-CH-DE",
    "art-lojban",
    "cel-gaulish",
    "no-bok",
    "no-nyn",
    "zh-guoyu",
    "zh-hakka",
    "zh-min",
    "zh-min-nan",
    "zh-xiang",
];

/// A BCP 47 language tag (RFC 5646), its primary subtag lower case, no
/// variant or extension repeated.
pub fn is_language(s: &str) -> bool {
    if GRANDFATHERED.contains(&s) {
        return true;
    }
    let tags: Vec<&str> = s.split('-').collect();
    let alpha = |t: &str, n: std::ops::RangeInclusive<usize>| {
        n.contains(&t.len()) && t.bytes().all(|c| c.is_ascii_alphabetic())
    };
    let alnum = |t: &str, n: std::ops::RangeInclusive<usize>| {
        n.contains(&t.len()) && t.bytes().all(|c| c.is_ascii_alphanumeric())
    };
    let private = |rest: &[&str]| !rest.is_empty() && rest.iter().all(|t| alnum(t, 1..=8));
    if tags[0].eq_ignore_ascii_case("x") {
        return private(&tags[1..]);
    }
    let primary = tags[0];
    if !(2..=3).contains(&primary.len()) || !primary.bytes().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    let mut i = 1;
    while i < tags.len() && i <= 3 && alpha(tags[i], 3..=3) {
        i += 1;
    }
    if i < tags.len() && alpha(tags[i], 4..=4) {
        i += 1;
    }
    let region = |t: &str| alpha(t, 2..=2) || t.len() == 3 && t.bytes().all(|c| c.is_ascii_digit());
    if i < tags.len() && region(tags[i]) {
        i += 1;
    }
    let variant = |t: &str| alnum(t, 5..=8) || alnum(t, 4..=4) && t.as_bytes()[0].is_ascii_digit();
    let mut seen = Vec::new();
    while i < tags.len() && variant(tags[i]) {
        seen.push(tags[i].to_ascii_lowercase());
        i += 1;
    }
    let singleton = |t: &str| alnum(t, 1..=1) && !t.eq_ignore_ascii_case("x");
    while i < tags.len() && singleton(tags[i]) {
        seen.push(tags[i].to_ascii_lowercase());
        let start = i + 1;
        i = start;
        while i < tags.len() && alnum(tags[i], 2..=8) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    let ended = i == tags.len() || tags[i].eq_ignore_ascii_case("x") && private(&tags[i + 1..]);
    ended && unique.len() == seen.len()
}
