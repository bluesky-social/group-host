//! The spaces a group's members make beside its two own: a calendar, a
//! forum, a channel. Each is the group's, with its access record saying
//! who reads it, who writes in it in their own name, and which roles
//! write in it as the group; and an entry in the members space's index,
//! which apps read to find it.
//!
//! Deleting a space ends it, as its authority, as well as taking it out
//! of the index.

use crate::Host;
use crate::api::invalid;
use crate::group::{members_space, meta_space};
use crate::group_api::{group_of, signed_in};
use crate::group_config::put_in;
use crate::rbac::{Access, NS, PUBLIC};
use crate::repo::Write;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn refused(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

/// The group a body names, once its caller of `lxm` may do `action` in it.
async fn acting(
    host: &Host,
    headers: &HeaderMap,
    body: &Value,
    lxm: &str,
    action: &str,
) -> Result<String, XrpcError> {
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(host, group)?;
    if !host.may_do(group, &me, action)? {
        return Err(forbidden(format!("missing {action}")));
    }
    Ok(group.to_string())
}

/// An audience list as given, `[{role}]`, if it is one.
fn audience(body: &Value, key: &str) -> Result<Option<Vec<Value>>, XrpcError> {
    let Some(given) = body.get(key) else {
        return Ok(None);
    };
    let roles = given
        .as_array()
        .expect("its lexicon has it a list of {role}");
    Ok(Some(roles.clone()))
}

/// The group's index entries of its spaces: key and record.
fn index(host: &Host, group: &str) -> Result<Vec<(String, Value)>, XrpcError> {
    host.group_records(group, &members_space(group), &format!("{NS}.space"))
        .map_err(XrpcError::internal)
}

/// The key of `space`'s entry in the group's index.
fn entry_of(host: &Host, group: &str, space: &str) -> Result<(String, Value), XrpcError> {
    let entries = index(host, group)?;
    let entry = entries.into_iter().find(|(_, e)| e["space"] == space);
    entry.ok_or_else(|| refused("SpaceNotFound", "the space is not in the group's index"))
}

/// The space a body names, the group's, and its access record: refused
/// for one kept for chosen people, which only they change.
fn own_space(host: &Host, group: &str, body: &Value) -> Result<(String, Value), XrpcError> {
    let space = body["space"].as_str().expect("its lexicon requires it");
    let named = SpaceRef::parse(space).map_err(|e| invalid(e.to_string()))?;
    if named.authority != group {
        return Err(invalid("the space is not this group's"));
    }
    let access = host
        .group_record(group, space, &format!("{NS}.access"), "self")
        .map_err(XrpcError::internal)?;
    let access = access.ok_or_else(|| refused("SpaceNotFound", "no such space"))?;
    if Access::of(&access).people.is_some() {
        return Err(forbidden(
            "that space is for chosen people: only they change it",
        ));
    }
    Ok((space.to_string(), access))
}

/// Adds a space to the group, for one holding `space.create`.
pub async fn create_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = acting(&host, &headers, &body, "createSpace", "space.create").await?;
    let kind = body["spaceType"].as_str().expect("its lexicon requires it");
    let readers = audience(&body, "readableBy")?.unwrap_or_default();
    if readers.is_empty() {
        return Err(invalid("readableBy names who reads the space"));
    }
    let writers = audience(&body, "writableBy")?;
    let skey = match body["skey"].as_str() {
        Some(skey) => skey.to_string(),
        None => host.clock.next(),
    };
    let uri = host.create_space(&group, kind, &skey, false)?;
    // A space without its access record or its entry could be neither
    // changed nor deleted, and its key never made again: it is made whole
    // or not at all.
    let furnished = furnish(&host, &group, &uri, &body, readers, writers).await;
    if let Err(e) = furnished {
        host.end_space(&group, &uri).map_err(XrpcError::internal)?;
        return Err(e);
    }
    Ok(Json(json!({ "uri": uri })))
}

/// Writes a space just made its access record, and its entry in the
/// group's index, as `body` has them.
async fn furnish(
    host: &Host,
    group: &str,
    uri: &str,
    body: &Value,
    readers: Vec<Value>,
    writers: Option<Vec<Value>>,
) -> Result<(), XrpcError> {
    let at = iso(now_millis());
    let mut access =
        json!({"$type": format!("{NS}.access"), "readableBy": readers, "createdAt": at});
    if let Some(writers) = writers.filter(|w| !w.is_empty()) {
        access["writableBy"] = json!(writers);
    }
    if let Some(scopes) = body.get("credentialScopes") {
        access["credentialScopes"] = scopes.clone();
    }
    let access_write = Write::Create {
        collection: format!("{NS}.access"),
        rkey: "self".into(),
        value: access,
    };
    host.space_write(group, uri, &[access_write]).await?;
    let mut entry = json!({"$type": format!("{NS}.space"), "space": uri, "createdAt": at});
    if let Some(name) = body["name"].as_str() {
        entry["name"] = json!(name);
    }
    let indexed = Write::Create {
        collection: format!("{NS}.space"),
        rkey: host.clock.next(),
        value: entry,
    };
    host.space_write(group, &members_space(group), &[indexed])
        .await?;
    Ok(())
}

/// Changes a space: its name, who reads it, who writes in it in their own
/// name, and which roles write in it as the group. For one holding
/// `space.configure`.
pub async fn update_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = acting(&host, &headers, &body, "updateSpace", "space.configure").await?;
    let (access_collection, space_collection) = (format!("{NS}.access"), format!("{NS}.space"));
    let (space, current) = own_space(&host, &group, &body)?;
    let (meta, members) = (meta_space(&group), members_space(&group));
    let readers = audience(&body, "readableBy")?;
    if readers.as_ref().is_some_and(Vec::is_empty) {
        return Err(invalid("readableBy names who reads the space"));
    }
    let writers = audience(&body, "writableBy")?;
    let name = body["name"].as_str();
    let names = |list: &[Value]| -> Vec<String> {
        list.iter()
            .filter_map(|r| r["role"].as_str().map(String::from))
            .collect()
    };
    if let Some(readers) = &readers {
        // The members space unreadable by members would lock the group out
        // of its own roster, whoever changes it included.
        if space == members && !names(readers).iter().any(|r| r == "member") {
            return Err(invalid("the members space stays readable by member"));
        }
    }
    // Writing in the members space in one's own name is joining: who may
    // stays as it is.
    if writers.is_some() && space == members {
        return Err(invalid("who may write in the members space is fixed"));
    }
    // The entry is found before anything is written, so a change refused
    // for want of it changes nothing.
    let named = match name {
        Some(name) => Some((name, entry_of(&host, &group, &space)?)),
        None => None,
    };
    if readers.is_some() || writers.is_some() || body.get("credentialScopes").is_some() {
        let mut access = current.clone();
        if let Some(readers) = &readers {
            access["readableBy"] = json!(readers);
        }
        if let Some(writers) = writers {
            let fields = access.as_object_mut().expect("an access record");
            fields.remove("writableBy");
            if !writers.is_empty() {
                fields.insert("writableBy".into(), json!(writers));
            }
        }
        if let Some(scopes) = body.get("credentialScopes") {
            access["credentialScopes"] = scopes.clone();
        }
        let put = put_in(&host, &group, &space, &access_collection, "self", access);
        let _written = put.await?;
    }
    // Meta's readers are the group's visibility: it is declared only while
    // anyone may read it, once the access record says so.
    if let Some(readers) = readers.as_ref().filter(|_| space == meta) {
        let declared = host.declare(&group, names(readers).iter().any(|r| r == PUBLIC));
        declared.await?;
    }
    if let Some((name, (rkey, mut entry))) = named {
        entry["name"] = json!(name);
        let put = put_in(&host, &group, &members, &space_collection, &rkey, entry);
        let _written = put.await?;
    }
    Ok(Json(json!({})))
}

/// Ends a space, for one holding `space.delete`: the services syncing it
/// are told, and its authority's state and the group's own repo in it go.
/// What members wrote there is theirs, at their own PDSes. Meta and
/// members stay: nothing works without them.
pub async fn delete_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = acting(&host, &headers, &body, "deleteSpace", "space.delete").await?;
    let (space, _) = own_space(&host, &group, &body)?;
    if space == meta_space(&group) || space == members_space(&group) {
        return Err(refused(
            "WellKnownSpace",
            "the meta and members spaces stay",
        ));
    }
    let (rkey, _) = entry_of(&host, &group, &space)?;
    let unindexed = Write::Delete {
        collection: format!("{NS}.space"),
        rkey,
    };
    host.space_write(&group, &members_space(&group), &[unindexed])
        .await?;
    host.end_space(&group, &space)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
