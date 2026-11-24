//! A group's face: its profile, its pictures and its rules, each a record
//! in its meta space that the group writes, and its declaration, in its
//! public repo, which lists it while anyone may read that space. An app
//! writes them as the group, with a token a member approved
//! (`token_writes`); here is what each must be for the host to keep it,
//! beyond what its lexicon says, and who may configure the group.

use crate::Host;
use crate::api::invalid;
use crate::blobs::PROFILE_IMAGES;
use crate::group::{join_policy, members_space, meta_space, rkey_did};
use crate::rbac::{NS, actions_for, roles_of};
use crate::repo::Write;
use crate::xrpc::XrpcError;
use atproto::Cid;
use atproto::syntax::is_tid;
use axum::Json;
use axum::http::StatusCode;
use serde_json::{Value, json};

type Answer = Result<Json<Value>, XrpcError>;

/// The most bytes a profile's picture may be, as its lexicon has it.
pub const IMAGE_AT_MOST: i64 = 2_000_000;

/// The most bytes a link's icon may be, as its lexicon has it.
pub const ICON_AT_MOST: i64 = 100_000;

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

impl Host {
    /// Whether `did` holds `group.configure` in `group`, by its records now.
    pub fn may_configure(&self, group: &str, did: &str) -> Result<bool, XrpcError> {
        Ok(self.configurers(group)?.iter().any(|c| c == did))
    }

    /// Whether `did`'s roles in `group` carry `action`.
    pub fn may_do(&self, group: &str, did: &str, action: &str) -> Result<bool, XrpcError> {
        let granted = self.role_grants(group).map_err(XrpcError::internal)?;
        let roles = self.roles(group, did).map_err(XrpcError::internal)?;
        Ok(actions_for(&granted, &roles).contains(action))
    }

    /// Everyone whose roles in `group` carry `group.configure`.
    pub fn configurers(&self, group: &str) -> Result<Vec<String>, XrpcError> {
        let members = members_space(group);
        let granted = self.role_grants(group).map_err(XrpcError::internal)?;
        let memberships = self
            .group_records(group, &members, &format!("{NS}.membership"))
            .map_err(XrpcError::internal)?;
        Ok(memberships
            .iter()
            .filter(|(_, m)| actions_for(&granted, &roles_of(m)).contains("group.configure"))
            .map(|(k, _)| rkey_did(k))
            .collect())
    }
}

/// Puts the group's record at `collection/rkey` in `space`, made or
/// replaced: its URI and CID.
pub(crate) async fn put_in(
    host: &Host,
    group: &str,
    space: &str,
    collection: &str,
    rkey: &str,
    value: Value,
) -> Answer {
    let current = host
        .group_record(group, space, collection, rkey)
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
    host.space_write(group, space, &[write]).await?;
    let kept = host
        .store
        .space_record(group, space, &collection, &rkey)
        .map_err(XrpcError::internal)?
        .expect("the record just written");
    let uri = format!("{space}/{group}/{collection}/{rkey}");
    Ok(Json(json!({"uri": uri, "cid": kept.cid.to_string()})))
}

/// Whether `text` says something: more than spaces.
fn said(text: &Value) -> bool {
    text.as_str().is_some_and(|t| !t.trim().is_empty())
}

/// Refuses the words a profile lists at `key`, as `[{field}]`, if one is
/// blank or listed twice.
fn words(profile: &Value, key: &str, field: &str) -> Result<(), XrpcError> {
    let mut seen: Vec<&str> = Vec::new();
    for item in profile[key].as_array().into_iter().flatten() {
        let word = item[field].as_str().unwrap_or_default().trim();
        if word.is_empty() || seen.contains(&word) {
            let once = format!("{key} lists each {field} once, and none blank");
            return Err(invalid(once));
        }
        seen.push(word);
    }
    Ok(())
}

/// Refuses a place the profile says it meets that is none of [`PLACES`].
fn place(profile: &Value) -> Result<(), XrpcError> {
    let Some(place) = profile.get("location") else {
        return Ok(());
    };
    let kind = place["$type"].as_str().unwrap_or_default();
    if !PLACES.contains(&kind) {
        return Err(invalid(format!("location is one of {}", PLACES.join(", "))));
    }
    Ok(())
}

/// Whether `url` is an address on the web.
fn on_the_web(url: &Value) -> bool {
    let url = url.as_str().unwrap_or_default();
    url.starts_with("https://") || url.starts_with("http://")
}

/// Refuses a site, or a link, that is not on the web.
fn site(profile: &Value) -> Result<(), XrpcError> {
    if profile.get("url").is_some_and(|url| !on_the_web(url)) {
        return Err(invalid("url is an http or https address"));
    }
    let links = profile["links"].as_array().into_iter().flatten();
    if links.into_iter().any(|link| !on_the_web(&link["url"])) {
        return Err(invalid("each link's url is an http or https address"));
    }
    Ok(())
}

impl Host {
    /// Refuses a picture a profile names at `key`, unless it is one of the
    /// group's own uploads, of a kind of image a page is given and no
    /// larger than a profile takes.
    fn picture(&self, group: &str, profile: &Value, key: &str) -> Result<(), XrpcError> {
        let Some(asked) = profile.get(key) else {
            return Ok(());
        };
        let not_one = format!(
            "{key} is a PNG, JPEG or WebP image of at most {IMAGE_AT_MOST} bytes, as the group's uploadBlob answered it"
        );
        self.image(group, asked, &PROFILE_IMAGES, IMAGE_AT_MOST, &not_one)
    }

    /// Refuses an icon a link names, unless it is one of the group's own
    /// uploads, a PNG no larger than a link takes.
    fn icons(&self, group: &str, profile: &Value) -> Result<(), XrpcError> {
        let not_one = format!(
            "a link's icon is a PNG image of at most {ICON_AT_MOST} bytes, as the group's uploadBlob answered it"
        );
        let links = profile["links"].as_array().into_iter().flatten();
        for icon in links.filter_map(|link| link.get("icon")) {
            self.image(group, icon, &["image/png"], ICON_AT_MOST, &not_one)?;
        }
        Ok(())
    }

    /// Refuses the blob `asked`, unless it is one of the group's own
    /// uploads, of one of `kinds` and at most `at_most` bytes, with the
    /// type and size the host kept: `not_one` says why.
    fn image(
        &self,
        group: &str,
        asked: &Value,
        kinds: &[&str],
        at_most: i64,
        not_one: &str,
    ) -> Result<(), XrpcError> {
        let not_one = || invalid(not_one.to_string());
        let link = asked["ref"]["$link"].as_str().unwrap_or_default();
        let cid = Cid::parse(link).map_err(|_| not_one())?;
        let kept = self.store.blob(group, &cid).map_err(XrpcError::internal)?;
        let kept = kept.ok_or_else(not_one)?;
        let image = kinds.contains(&kept.mime_type.as_str());
        let as_kept = asked["mimeType"] == kept.mime_type && asked["size"] == kept.size;
        if !image || kept.size > at_most || !as_kept {
            return Err(not_one());
        }
        Ok(())
    }

    /// Refuses a profile the group should not keep: one with no name, a
    /// join policy the host does not know, a place that is none, a site or
    /// link not on the web, words blank or listed twice, or a picture or a
    /// link's icon that is not one of its own.
    fn profile(&self, group: &str, profile: &Value) -> Result<(), XrpcError> {
        if !said(&profile["displayName"]) {
            return Err(invalid("displayName is required"));
        }
        if let Some(policy) = profile["joinPolicy"].as_str() {
            join_policy(Some(policy))?;
        }
        place(profile)?;
        site(profile)?;
        words(profile, "topics", "topic")?;
        words(profile, "languages", "language")?;
        self.picture(group, profile, "avatar")?;
        self.picture(group, profile, "banner")?;
        self.icons(group, profile)
    }

    /// Refuses a write of the group's profile or one of its rules, in its
    /// meta space, that the host should not keep: a profile as `profile`
    /// has it, kept at `self` and never deleted; a rule with a title, at a
    /// TID, so that a label citing it keeps its URI however it changes.
    pub(crate) fn face(&self, group: &str, write: &Write) -> Result<(), XrpcError> {
        let (collection, rkey) = write.path();
        let value = match write {
            Write::Create { value, .. } | Write::Update { value, .. } => Some(value),
            Write::Delete { .. } => None,
        };
        if collection == format!("{NS}.profile") {
            if rkey != "self" {
                return Err(invalid(format!("{collection} is kept at self")));
            }
            let kept = || invalid("the group keeps its profile");
            return self.profile(group, value.ok_or_else(kept)?);
        }
        if let Some(rule) = value.filter(|_| collection == format!("{NS}.rule")) {
            if !is_tid(rkey) {
                return Err(invalid("a rule is kept at a TID"));
            }
            if !said(&rule["title"]) {
                return Err(invalid("title is required"));
            }
        }
        Ok(())
    }

    /// Refuses a write of the group's declaration, in its public repo,
    /// that does not follow who may read its meta space: it is kept at
    /// `self`, names that space, and is made only while anyone may read
    /// it. Taking it away is always the group's to do.
    pub(crate) fn declaration(&self, group: &str, write: &Write) -> Result<(), XrpcError> {
        let (collection, rkey) = write.path();
        if collection != format!("{NS}.declaration") {
            return Ok(());
        }
        if rkey != "self" {
            return Err(invalid(format!("{collection} is kept at self")));
        }
        let (Write::Create { value, .. } | Write::Update { value, .. }) = write else {
            return Ok(());
        };
        let meta = meta_space(group);
        if value["meta"] != meta.as_str() {
            let named = format!("a group's declaration names its meta space, {meta}");
            return Err(invalid(named));
        }
        let public = self.hosted_space(&meta)?.public_read;
        if !public {
            let hidden = "a group is declared only while anyone may read its meta space";
            return Err(invalid(hidden));
        }
        Ok(())
    }
}
