//! The spaces a group's members make beside its two own: a calendar, a
//! forum, a channel. Each is the group's, with its access record granting
//! each role scopes in it (`read` among them, which lets a member write
//! there in their own name too), or making it public; and, unless it is
//! left out, an entry in the members space's index, which apps read to
//! find it.
//!
//! Deleting a space ends it, as its authority, as well as taking it out
//! of the index.

use crate::Host;
use crate::api::invalid;
use crate::group::{members_space, meta_space};
use crate::group_api::{group_of, signed_in};
use crate::group_config::put_in;
use crate::oauth_scope::SpaceScope;
use crate::rbac::{Access, NS, SpaceGrant};
use crate::repo::Write;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::BTreeSet;
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

/// The access record `config` (an `accessConfig`) asks for in the space
/// at `uri`, checked: one grant a role, each of a role the group has and
/// of space scopes, and someone besides the group to read the space.
fn access_record(host: &Host, group: &str, uri: &str, config: &Value) -> Result<Value, XrpcError> {
    let grants = config["grants"]
        .as_array()
        .expect("its lexicon requires it");
    let roles = host.group_records(group, &members_space(group), &format!("{NS}.role"));
    let roles = roles.map_err(XrpcError::internal)?;
    let mut seen = BTreeSet::new();
    for grant in grants {
        let role = grant["role"].as_str().expect("its lexicon requires it");
        if !seen.insert(role) {
            return Err(invalid(format!("{role} has more than one grant")));
        }
        if !roles.iter().any(|(r, _)| r == role) {
            return Err(refused(
                "UnknownRole",
                format!("the group has no role {role}"),
            ));
        }
        let scopes = grant["scopes"].as_array().expect("its lexicon requires it");
        for scope in scopes.iter().filter_map(Value::as_str) {
            if SpaceScope::parse(scope).is_none() {
                return Err(refused(
                    "InvalidScope",
                    format!("{scope} is no space scope"),
                ));
            }
        }
    }
    let mut record = json!({"$type": format!("{NS}.access"), "grants": grants});
    if config["public"] == true {
        record["public"] = json!(true);
    }
    let writers = config["writableBy"].as_array().filter(|w| !w.is_empty());
    if let Some(writers) = writers {
        record["writableBy"] = json!(writers);
    }
    let access = Access::of(&record, uri);
    let read = |g: &SpaceGrant| access.reads(std::slice::from_ref(&g.role));
    if !access.public && !access.grants.iter().any(read) {
        return Err(invalid("nobody but the group would read the space"));
    }
    Ok(record)
}

/// A record of the group's just written in `space`, as a strong reference.
fn strong_ref(
    host: &Host,
    group: &str,
    space: &str,
    collection: &str,
    rkey: &str,
) -> Result<Value, XrpcError> {
    let kept = host.store.space_record(group, space, collection, rkey);
    let kept = kept.map_err(XrpcError::internal)?;
    let cid = kept.expect("the record just written").cid.to_string();
    let uri = format!("{space}/{group}/{collection}/{rkey}");
    Ok(json!({"uri": uri, "cid": cid}))
}

/// `space`'s entry in the group's index, if it has one: key and record.
fn entry_of(host: &Host, group: &str, space: &str) -> Result<Option<(String, Value)>, XrpcError> {
    let entries = host.group_records(group, &members_space(group), &format!("{NS}.space"));
    let entries = entries.map_err(XrpcError::internal)?;
    Ok(entries.into_iter().find(|(_, e)| e["space"] == space))
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
    if Access::of(&access, space).people.is_some() {
        return Err(forbidden(
            "that space is for chosen people: only they change it",
        ));
    }
    Ok((space.to_string(), access))
}

/// Refused for the meta and members spaces, which every group has.
fn reserved_space() -> XrpcError {
    refused(
        "ReservedSpace",
        "the meta and members spaces come and go with the group",
    )
}

/// Adds a space to the group, for one holding `space.create`: with its
/// access record, and an entry in the group's index unless `index` is
/// false.
pub async fn create_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = acting(&host, &headers, &body, "createSpace", "space.create").await?;
    let kind = body["type"].as_str().expect("its lexicon requires it");
    if [format!("{NS}.meta"), format!("{NS}.members")].contains(&kind.to_string()) {
        return Err(reserved_space());
    }
    let skey = match body["skey"].as_str() {
        Some(skey) => skey.to_string(),
        None => host.clock.next(),
    };
    let uri = format!("at://{group}/space/{kind}/{skey}");
    let access = access_record(&host, &group, &uri, &body["access"])?;
    let uri = host.create_space(&group, kind, &skey, false)?;
    // A space without its access record could be neither changed nor
    // deleted, and its key never made again: it is made whole or not at
    // all.
    match furnish(&host, &group, &uri, &body, access).await {
        Ok(made) => Ok(Json(made)),
        Err(e) => {
            host.end_space(&group, &uri).map_err(XrpcError::internal)?;
            Err(e)
        }
    }
}

/// Writes a space just made its access record, and its entry in the
/// group's index unless `body` says not: what was written, as
/// `createSpace` answers it.
async fn furnish(
    host: &Host,
    group: &str,
    uri: &str,
    body: &Value,
    access: Value,
) -> Result<Value, XrpcError> {
    let collection = format!("{NS}.access");
    let access_write = Write::Create {
        collection: collection.clone(),
        rkey: "self".into(),
        value: access,
    };
    host.space_write(group, uri, &[access_write]).await?;
    let access = strong_ref(host, group, uri, &collection, "self")?;
    let mut made = json!({"space": uri, "access": access});
    if body["index"] == false {
        return Ok(made);
    }
    let at = iso(now_millis());
    let mut entry = json!({"$type": format!("{NS}.space"), "space": uri, "createdAt": at});
    if let Some(name) = body["name"].as_str() {
        entry["name"] = json!(name);
    }
    let (members, rkey) = (members_space(group), host.clock.next());
    let collection = format!("{NS}.space");
    let indexed = Write::Create {
        collection: collection.clone(),
        rkey: rkey.clone(),
        value: entry,
    };
    host.space_write(group, &members, &[indexed]).await?;
    made["index"] = strong_ref(host, group, &members, &collection, &rkey)?;
    Ok(made)
}

/// Changes a space: its name, in its index entry, and its access record,
/// replaced whole. For one holding `space.configure`.
pub async fn update_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = acting(&host, &headers, &body, "updateSpace", "space.configure").await?;
    let (access_collection, space_collection) = (format!("{NS}.access"), format!("{NS}.space"));
    let (space, _) = own_space(&host, &group, &body)?;
    let (meta, members) = (meta_space(&group), members_space(&group));
    let access = match body.get("access") {
        Some(config) => Some(access_record(&host, &group, &space, config)?),
        None => None,
    };
    if let Some(access) = access.as_ref().filter(|_| space == members) {
        // The members space unreadable by members would lock the group out
        // of its own roster, whoever changes it included.
        if !Access::of(access, &members).reads(&["member".into()]) {
            return Err(invalid("the members space stays readable by member"));
        }
        // Writing in the members space in one's own name is joining: who
        // may stays as it is.
        if access.get("writableBy").is_some() {
            return Err(invalid("who may write in the members space is fixed"));
        }
    }
    // The entry is found before anything is written, so a change refused
    // for want of it changes nothing.
    let named = match body["name"].as_str() {
        Some(name) => {
            let entry = entry_of(&host, &group, &space)?;
            let unlisted = || refused("SpaceNotFound", "the space is not in the group's index");
            Some((name, entry.ok_or_else(unlisted)?))
        }
        None => None,
    };
    if let Some(access) = access {
        let public = access["public"] == true;
        let put = put_in(&host, &group, &space, &access_collection, "self", access);
        let _written = put.await?;
        // Meta's readers are the group's visibility: it is declared only
        // while anyone may read it.
        if space == meta {
            host.declare(&group, public).await?;
        }
    }
    if let Some((name, (rkey, mut entry))) = named {
        entry["name"] = json!(name);
        let put = put_in(&host, &group, &members, &space_collection, &rkey, entry);
        let _written = put.await?;
    }
    Ok(Json(json!({})))
}

/// Ends a space, for one holding `space.delete`: it leaves the index if it
/// is there, the services syncing it are told, and its authority's state
/// and the group's own repo in it go. What members wrote there is theirs,
/// at their own PDSes. Meta and members stay: nothing works without them.
pub async fn delete_space(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = acting(&host, &headers, &body, "deleteSpace", "space.delete").await?;
    let (space, _) = own_space(&host, &group, &body)?;
    if space == meta_space(&group) || space == members_space(&group) {
        return Err(reserved_space());
    }
    if let Some((rkey, _)) = entry_of(&host, &group, &space)? {
        let unindexed = Write::Delete {
            collection: format!("{NS}.space"),
            rkey,
        };
        host.space_write(&group, &members_space(&group), &[unindexed])
            .await?;
    }
    host.end_space(&group, &space)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}
