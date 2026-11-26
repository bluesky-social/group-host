//! A group's face: its about record and its picture, and its config, each
//! a record in its meta space that the group writes; its rules and
//! labeler, in its members space; and its declaration, in its public
//! repo, which lists it while anyone may read its meta space. An app
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

/// The most bytes the group's picture may be, as its lexicon has it.
pub const IMAGE_AT_MOST: i64 = 2_000_000;

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

/// Refuses a website that is not on the web.
fn website(about: &Value) -> Result<(), XrpcError> {
    let website = about
        .get("website")
        .map(|website| website.as_str().unwrap_or_default());
    let web = |w: &str| w.starts_with("https://") || w.starts_with("http://");
    if website.is_some_and(|w| !web(w)) {
        return Err(invalid("website is an http or https address"));
    }
    Ok(())
}

impl Host {
    /// Refuses a picture an about record names at `key`, unless it is one
    /// of the group's own uploads, of a kind of image a page is given and
    /// no larger than the record takes, with the type and size the host
    /// kept.
    fn picture(&self, group: &str, about: &Value, key: &str) -> Result<(), XrpcError> {
        let Some(asked) = about.get(key) else {
            return Ok(());
        };
        let not_one = || {
            invalid(format!(
                "{key} is a PNG, JPEG or WebP image of at most {IMAGE_AT_MOST} bytes, as the group's uploadBlob answered it"
            ))
        };
        let link = asked["ref"]["$link"].as_str().unwrap_or_default();
        let cid = Cid::parse(link).map_err(|_| not_one())?;
        let kept = self.store.blob(group, &cid).map_err(XrpcError::internal)?;
        let kept = kept.ok_or_else(not_one)?;
        let image = PROFILE_IMAGES.contains(&kept.mime_type.as_str());
        let as_kept = asked["mimeType"] == kept.mime_type && asked["size"] == kept.size;
        if !image || kept.size > IMAGE_AT_MOST || !as_kept {
            return Err(not_one());
        }
        Ok(())
    }

    /// Refuses an about record the group should not keep: one with no
    /// name, a website not on the web, or a picture that is not one of its
    /// own.
    fn about(&self, group: &str, about: &Value) -> Result<(), XrpcError> {
        if !said(&about["displayName"]) {
            return Err(invalid("displayName is required"));
        }
        website(about)?;
        self.picture(group, about, "avatar")
    }

    /// Refuses a write of the group's own records in its meta or members
    /// space that the host should not keep: its about record (as `about`
    /// has it), config (with a join policy the host knows) and labeler,
    /// each kept at `self` and never deleted; a rule with a title, at a
    /// TID, so that a label citing it keeps its URI however it changes.
    pub(crate) fn face(&self, group: &str, write: &Write) -> Result<(), XrpcError> {
        let (collection, rkey) = write.path();
        let value = match write {
            Write::Create { value, .. } | Write::Update { value, .. } => Some(value),
            Write::Delete { .. } => None,
        };
        let name = collection
            .strip_prefix(&format!("{NS}."))
            .unwrap_or_default();
        if ["about", "config", "labeler"].contains(&name) {
            if rkey != "self" {
                return Err(invalid(format!("{collection} is kept at self")));
            }
            let kept = || invalid(format!("the group keeps its {collection}"));
            let value = value.ok_or_else(kept)?;
            return match name {
                "about" => self.about(group, value),
                "config" => join_policy(value["joinPolicy"].as_str()).map(|_| ()),
                _ => Ok(()),
            };
        }
        if let Some(rule) = value.filter(|_| name == "rule") {
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
    /// `self`, and made only while anyone may read that space. Taking it
    /// away is always the group's to do.
    pub(crate) fn declaration(&self, group: &str, write: &Write) -> Result<(), XrpcError> {
        let (collection, rkey) = write.path();
        if collection != format!("{NS}.declaration") {
            return Ok(());
        }
        if rkey != "self" {
            return Err(invalid(format!("{collection} is kept at self")));
        }
        if matches!(write, Write::Delete { .. }) {
            return Ok(());
        }
        let meta = meta_space(group);
        let public = self.hosted_space(&meta)?.public_read;
        if !public {
            let hidden = "a group is declared only while anyone may read its meta space";
            return Err(invalid(hidden));
        }
        Ok(())
    }
}
