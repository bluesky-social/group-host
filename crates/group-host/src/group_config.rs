//! Changing a group's face: its profile, its images and its rules. Each is
//! a record in its meta space, written by the group, so the host writes it
//! for a caller holding `group.configure`.
//!
//! As the prototype's `methods/profile.ts` and `methods/rules.ts`.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::group::{join_policy, members_space, meta_space};
use crate::group_api::{group_of, signed_in};
use crate::rbac::{NS, actions_for, grants, listed};
use crate::repo::Write;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// The most bytes an image may be, as the profile's lexicon has it.
pub const IMAGE_AT_MOST: usize = 1_000_000;

/// Where a group meets: the places `community.lexicon.location` has.
const PLACES: [&str; 4] = [
    "community.lexicon.location.address",
    "community.lexicon.location.geo",
    "community.lexicon.location.fsq",
    "community.lexicon.location.hthree",
];

pub(crate) fn forbidden() -> XrpcError {
    XrpcError::new(
        StatusCode::FORBIDDEN,
        "Forbidden",
        "missing group.configure",
    )
}

/// The group a caller of `lxm` changes, once they hold `group.configure`
/// in it.
async fn configurer(
    host: &Host,
    headers: &HeaderMap,
    group: &str,
    lxm: &str,
) -> Result<String, XrpcError> {
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    group_of(host, group)?;
    if !host.may_configure(group, &me)? {
        return Err(forbidden());
    }
    Ok(group.to_string())
}

impl Host {
    /// Whether `did` holds `group.configure` in `group`, by its records now.
    pub fn may_configure(&self, group: &str, did: &str) -> Result<bool, XrpcError> {
        Ok(self.configurers(group)?.iter().any(|c| c == did))
    }

    /// Everyone whose roles in `group` carry `group.configure`.
    pub fn configurers(&self, group: &str) -> Result<Vec<String>, XrpcError> {
        let members = members_space(group);
        let permissions = self
            .group_record(group, &members, &format!("{NS}.permissions"), "self")
            .map_err(XrpcError::internal)?;
        let granted = grants(&permissions.unwrap_or_default());
        let memberships = self
            .group_records(group, &members, &format!("{NS}.membership"))
            .map_err(XrpcError::internal)?;
        Ok(memberships
            .iter()
            .filter(|(_, m)| {
                actions_for(&granted, &listed(m, "roles", "role")).contains("group.configure")
            })
            .filter_map(|(_, m)| m["member"].as_str().map(String::from))
            .collect())
    }
}

/// Puts the group's record at `collection/rkey` in meta, made or replaced:
/// its URI and CID.
async fn put(host: &Host, group: &str, collection: &str, rkey: &str, value: Value) -> Answer {
    let meta = meta_space(group);
    let current = host
        .group_record(group, &meta, collection, rkey)
        .map_err(XrpcError::internal)?;
    let (collection, rkey) = (collection.to_string(), rkey.to_string());
    let write = match current {
        Some(_) => Write::Update {
            collection: collection.clone(),
            rkey: rkey.clone(),
            value,
        },
        None => Write::Create {
            collection: collection.clone(),
            rkey: rkey.clone(),
            value,
        },
    };
    host.space_write(group, &meta, &[write]).await?;
    let kept = host
        .store
        .space_record(group, &meta, &collection, &rkey)
        .map_err(XrpcError::internal)?
        .expect("the record just written");
    let uri = format!("{meta}/{group}/{collection}/{rkey}");
    Ok(Json(json!({"uri": uri, "cid": kept.cid.to_string()})))
}

/// Words a profile lists, as `[{topic}]`: trimmed, each once, at most
/// `max`.
fn words(asked: &Value, field: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in asked.as_array().into_iter().flatten() {
        let word = item[field].as_str().unwrap_or_default().trim().to_string();
        if !word.is_empty() && !out.contains(&word) {
            out.push(word);
        }
    }
    out.truncate(max);
    out
}

/// A BCP 47 tag, as the profile's languages are.
fn is_language(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let first = parts.next().unwrap_or_default();
    (2..=3).contains(&first.len())
        && first.chars().all(|c| c.is_ascii_alphabetic())
        && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// A blob as a record holds it, `{$type, ref, mimeType, size}`, from what
/// `uploadImage` answered.
fn blob(asked: &Value) -> Option<Value> {
    let link = asked["ref"]["$link"].as_str()?;
    let mime = asked["mimeType"].as_str()?;
    let size = asked["size"].as_u64()?;
    Some(json!({"$type": "blob", "ref": {"$link": link}, "mimeType": mime, "size": size}))
}

/// Replaces the group's profile. What finds it (where it meets, online,
/// topics, languages) is as given, none when left out; an image left out
/// is kept, and `null` clears it; its first `createdAt` is kept.
pub async fn update_profile(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    let group = configurer(&host, &headers, group, "updateProfile").await?;
    let name = body["displayName"].as_str().unwrap_or_default().trim();
    if name.is_empty() {
        return Err(invalid("displayName is required"));
    }
    let meta = meta_space(&group);
    let collection = format!("{NS}.profile");
    let current = host
        .group_record(&group, &meta, &collection, "self")
        .map_err(XrpcError::internal)?
        .unwrap_or_default();
    let join = match body["joinPolicy"].as_str() {
        Some(asked) => join_policy(Some(asked))?.to_string(),
        None => current["joinPolicy"]
            .as_str()
            .unwrap_or("approval")
            .to_string(),
    };
    let mut profile = json!({
        "$type": collection,
        "displayName": name,
        "joinPolicy": join,
        "createdAt": current.get("createdAt").cloned().unwrap_or(json!(iso(now_millis()))),
    });
    let text = |k: &str| body[k].as_str().map(str::trim).filter(|t| !t.is_empty());
    if let Some(d) = text("description") {
        profile["description"] = json!(d);
    }
    // The site is kept, like the images: an edit of the name says nothing
    // of it.
    match text("url") {
        Some(url) => profile["url"] = json!(url),
        None => {
            if let Some(url) = current.get("url") {
                profile["url"] = url.clone();
            }
        }
    }
    if let Some(place) = body.get("location").filter(|l| !l.is_null()) {
        let kind = place["$type"].as_str().unwrap_or_default();
        if !PLACES.contains(&kind) {
            return Err(invalid(format!("location is one of {}", PLACES.join(", "))));
        }
        profile["location"] = place.clone();
    }
    if body["online"].as_bool() == Some(true) {
        profile["online"] = json!(true);
    }
    let topics: Vec<Value> = words(&body["topics"], "topic", 10)
        .into_iter()
        .map(|t| json!({"topic": t.chars().take(64).collect::<String>()}))
        .collect();
    if !topics.is_empty() {
        profile["topics"] = json!(topics);
    }
    let languages: Vec<Value> = words(&body["languages"], "language", 5)
        .into_iter()
        .filter(|l| is_language(l))
        .map(|l| json!({"language": l}))
        .collect();
    if !languages.is_empty() {
        profile["languages"] = json!(languages);
    }
    for key in ["avatar", "banner"] {
        let image = match body.get(key) {
            Some(Value::Null) => None,
            Some(asked) => Some(blob(asked).ok_or_else(|| invalid(format!("{key} is a blob")))?),
            None => current.get(key).and_then(blob),
        };
        if let Some(image) = image {
            profile[key] = image;
        }
    }
    put(&host, &group, &collection, "self", profile).await
}

/// Keeps an image as the group's, for its profile: the blob to put there.
pub async fn upload_image(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Answer {
    let group = configurer(&host, &headers, param(&p, "group")?, "uploadImage").await?;
    if bytes.is_empty() {
        return Err(invalid("an empty upload"));
    }
    if bytes.len() > IMAGE_AT_MOST {
        return Err(invalid(format!(
            "an image is {IMAGE_AT_MOST} bytes at most"
        )));
    }
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let blob = host.upload_blob(&group, &bytes, mime)?;
    Ok(Json(json!({ "blob": blob })))
}

/// Makes or changes a rule. Its key stays as it is, so a label citing it
/// still does; a new one gets a key of its own, and goes last unless it
/// says where.
pub async fn put_rule(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    let group = configurer(&host, &headers, group, "putRule").await?;
    let title = body["title"].as_str().unwrap_or_default().trim();
    if title.is_empty() {
        return Err(invalid("title is required"));
    }
    let rkey = match body["rkey"]
        .as_str()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        Some(k) => k.to_string(),
        None => host.clock.next(),
    };
    let collection = format!("{NS}.rule");
    let rules = host
        .group_records(&group, &meta_space(&group), &collection)
        .map_err(XrpcError::internal)?;
    let current = rules.iter().find(|(k, _)| *k == rkey).map(|(_, r)| r);
    let last = rules.iter().filter_map(|(_, r)| r["order"].as_i64()).max();
    let order = body["order"]
        .as_i64()
        .or_else(|| current.and_then(|r| r["order"].as_i64()))
        .unwrap_or_else(|| last.map_or(0, |l| l + 1));
    if order < 0 {
        return Err(invalid("order is 0 or more"));
    }
    let created = current
        .and_then(|r| r.get("createdAt").cloned())
        .unwrap_or_else(|| json!(iso(now_millis())));
    let rule = json!({
        "$type": collection,
        "title": title,
        "text": body["text"].as_str().unwrap_or_default().trim(),
        "order": order,
        "createdAt": created,
    });
    put(&host, &group, &collection, &rkey, rule).await
}

/// Takes a rule away. A label citing it keeps its URI, which no longer
/// resolves: better than a citation taken out of history.
pub async fn delete_rule(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    let group = configurer(&host, &headers, group, "deleteRule").await?;
    let rkey = body["rkey"]
        .as_str()
        .ok_or_else(|| invalid("rkey is required"))?;
    let collection = format!("{NS}.rule");
    let meta = meta_space(&group);
    let current = host
        .group_record(&group, &meta, &collection, rkey)
        .map_err(XrpcError::internal)?;
    if current.is_some() {
        let delete = Write::Delete {
            collection,
            rkey: rkey.into(),
        };
        host.space_write(&group, &meta, &[delete]).await?;
    }
    Ok(Json(json!({})))
}
