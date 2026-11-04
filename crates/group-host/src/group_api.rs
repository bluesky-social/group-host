//! The `fyi.opensocial.*` methods that make and read groups: starting
//! one, a group as its viewer may see it, and every group here.
//!
//! A caller is who their service auth says, for this host (`SERVICE_DID`)
//! and the method: their own PDS signs it for them.

use crate::Host;
use crate::api::{Params, invalid, param};
use crate::group::{Made, NewGroup, members_space, meta_space};
use crate::rbac::{self, Access, NS, actions_for, grants, listed, repo_collections_for};
use crate::space_api::{service_caller, unauthorized};
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// Who calls `lxm`: by their service auth for this host, or else the
/// console's session cookie; none without either.
pub(crate) async fn caller(
    host: &Host,
    headers: &HeaderMap,
    lxm: &str,
) -> Result<Option<String>, XrpcError> {
    // Signed in to the console, with no other say.
    if !headers.contains_key(header::AUTHORIZATION) {
        return host.console_caller(headers).map_err(XrpcError::internal);
    }
    let claims = service_caller(host, headers, lxm).await?;
    let ours = &host.config.service_did;
    let for_us = claims.aud == *ours
        || claims
            .aud
            .strip_prefix(ours.as_str())
            .is_some_and(|f| f.starts_with('#'));
    if !for_us {
        return Err(unauthorized(
            "BadJwtAudience",
            format!("a token for {}, not this host", claims.aud),
        ));
    }
    Ok(Some(claims.account().to_string()))
}

/// Who calls `lxm`, who must say.
pub(crate) async fn signed_in(
    host: &Host,
    headers: &HeaderMap,
    lxm: &str,
) -> Result<String, XrpcError> {
    let who = caller(host, headers, lxm).await?;
    who.ok_or_else(|| unauthorized("AuthMissing", "service auth is required"))
}

fn strings(body: &Value, key: &str, field: &str) -> Option<Vec<String>> {
    body.get(key).map(|_| listed(body, key, field))
}

/// The group a `createGroup` body asks for, `founder`'s.
pub(crate) fn new_group(body: &Value, founder: String) -> Result<NewGroup, XrpcError> {
    let text = |k: &str| body[k].as_str().map(String::from);
    Ok(NewGroup {
        handle: text("handle").ok_or_else(|| invalid("handle is required"))?,
        founder,
        stewards: strings(body, "stewards", "did").unwrap_or_default(),
        display_name: text("displayName").ok_or_else(|| invalid("displayName is required"))?,
        description: text("description"),
        join_policy: text("joinPolicy"),
        meta_readable_by: strings(body, "metaReadableBy", "role"),
    })
}

/// Starts a group, the caller its founder and first admin.
pub async fn create_group(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let founder = signed_in(&host, &headers, &format!("{NS}.createGroup")).await?;
    let Made { did, meta, members } = host.create_group(&new_group(&body, founder)?).await?;
    Ok(Json(json!({"did": did, "meta": meta, "members": members})))
}

pub(crate) fn not_found() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "GroupNotFound",
        "no such group here",
    )
}

/// A group here: its handle, if `did` is one.
pub(crate) fn group_of(host: &Host, did: &str) -> Result<String, XrpcError> {
    let account = host.store.account(did).map_err(XrpcError::internal)?;
    let account = account.filter(|a| a.did == did).ok_or_else(not_found)?;
    let space = host
        .store
        .space(&members_space(did))
        .map_err(XrpcError::internal)?;
    space.map(|_| account.handle).ok_or_else(not_found)
}

/// The CID a blob reference names.
fn cid_of(blob: &Value) -> Option<&str> {
    blob["ref"]["$link"].as_str()
}

/// What a group's profile shows of it.
fn profile_view(profile: &Value) -> Value {
    let mut view = json!({});
    for key in [
        "displayName",
        "description",
        "joinPolicy",
        "url",
        "location",
        "online",
        "topics",
        "languages",
    ] {
        if let Some(v) = profile.get(key) {
            view[key] = v.clone();
        }
    }
    for key in ["avatar", "banner"] {
        if let Some(cid) = cid_of(&profile[key]) {
            view[key] = json!(cid);
        }
    }
    view
}

/// The group's rules in the order they are presented: by `order`, then as
/// written, then by key.
fn rules(host: &Host, group: &str) -> Result<Vec<Value>, XrpcError> {
    let mut rules = host
        .group_records(group, &meta_space(group), &format!("{NS}.rule"))
        .map_err(XrpcError::internal)?;
    rules.sort_by(|(ak, a), (bk, b)| {
        let order = |r: &Value| r["order"].as_i64().unwrap_or(i64::MAX);
        let at = |r: &Value| r["createdAt"].as_str().unwrap_or_default().to_string();
        (order(a), at(a), ak).cmp(&(order(b), at(b), bk))
    });
    Ok(rules
        .into_iter()
        .map(|(rkey, r)| {
            let mut rule = json!({"rkey": rkey, "title": r["title"]});
            for key in ["text", "createdAt"] {
                if let Some(v) = r.get(key) {
                    rule[key] = v.clone();
                }
            }
            rule
        })
        .collect())
}

fn base(host: &Host, did: &str, handle: &str) -> Result<Value, XrpcError> {
    let address = host.store.address(did).map_err(XrpcError::internal)?;
    Ok(json!({
        "did": did,
        "handle": handle,
        "address": address,
        "meta": meta_space(did),
        "members": members_space(did),
    }))
}

fn merge(into: &mut Value, from: Value) {
    if let (Some(into), Value::Object(from)) = (into.as_object_mut(), from) {
        into.extend(from);
    }
}

fn objects(field: &str, items: impl IntoIterator<Item = impl Into<String>>) -> Value {
    items
        .into_iter()
        .map(|i| json!({ field: i.into() }))
        .collect()
}

/// One group in full, as the caller may see it: what its meta space
/// holds for whoever may read it, the caller's own standing, and for a
/// member, its members, roles and spaces.
pub async fn get_group(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    let handle = group_of(&host, group)?;
    let me = caller(&host, &headers, &format!("{NS}.getGroup")).await?;
    // Switched off: nothing in it is read, by members either. Still
    // answered, so a steward can find the switch.
    if host.deactivation(group)?.is_some() {
        let mut view = base(&host, group, &handle)?;
        view["deactivated"] = json!(true);
        return Ok(Json(json!({ "group": view })));
    }
    let record = |space: &str, collection: &str, rkey: &str| {
        host.group_record(group, space, &format!("{NS}.{collection}"), rkey)
            .map_err(XrpcError::internal)
    };
    let (meta, members) = (meta_space(group), members_space(group));
    let roles = match &me {
        Some(me) => host.roles(group, me).map_err(XrpcError::internal)?,
        None => Vec::new(),
    };
    let me_or = me.as_deref().unwrap_or_default();
    let reads = |space: &str| -> Result<bool, XrpcError> {
        let access = record(space, "access", "self")?;
        Ok(access.is_some_and(|a| Access::of(&a).can_read(&roles, me_or)))
    };
    let reads_meta = reads(&meta)?;
    let reads_members = me.is_some() && reads(&members)?;
    let permissions = record(&members, "permissions", "self")?.unwrap_or_default();
    let granted = grants(&permissions);
    let actions = actions_for(&granted, &roles);

    let mut view = base(&host, group, &handle)?;
    if reads_meta {
        if let Some(profile) = record(&meta, "profile", "self")? {
            merge(&mut view, profile_view(&profile));
        }
        view["rules"] = json!(rules(&host, group)?);
    }
    if let Some(me) = &me {
        let member = !roles.is_empty();
        let mut viewer = json!({
            "member": member,
            "roles": objects("role", roles.clone()),
            "actions": actions,
        });
        if member {
            viewer["accepted"] = json!(accepted(&host, &members, me).map_err(XrpcError::internal)?);
        }
        // Their own request, while it waits.
        let asked = host.store.asked(group, me).map_err(XrpcError::internal)?;
        if let Some(request) = asked.filter(|_| !member) {
            viewer["request"] = crate::group_join::request_view(&request);
        }
        view["viewer"] = viewer;
    }
    let mut out = json!({ "group": view });
    if !reads_members {
        return Ok(Json(out));
    }
    let records = |collection: &str| {
        host.group_records(group, &members, &format!("{NS}.{collection}"))
            .map_err(XrpcError::internal)
    };
    out["members"] = records("membership")?
        .into_iter()
        .map(|(_, m)| json!({"actor": {"did": m["member"]}, "roles": objects("role", listed(&m, "roles", "role"))}))
        .collect();
    out["roles"] = records("role")?
        .into_iter()
        .map(|(key, r)| {
            let mut role = json!({"key": key});
            for (from, to) in [
                ("displayName", "name"),
                ("description", "description"),
                ("bot", "bot"),
            ] {
                if let Some(v) = r.get(from) {
                    role[to] = v.clone();
                }
            }
            role
        })
        .collect();
    out["bindings"] = granted
        .iter()
        .map(|g| {
            let one = std::slice::from_ref(&g.role);
            json!({
                "role": g.role,
                "actions": actions_for(&granted, one),
                "assignable": objects("role", g.assignable.clone()),
                "repoCollections": objects("collection", repo_collections_for(&granted, one)),
                "scopes": objects("scope", g.scopes.clone()),
            })
        })
        .collect();
    let mut spaces = Vec::new();
    for (_, entry) in records("space")? {
        let uri = entry["space"].as_str().unwrap_or_default();
        let kind = uri.split('/').nth(4).unwrap_or_default();
        let mut space = json!({"uri": uri, "spaceType": kind});
        if let Some(name) = entry.get("name") {
            space["name"] = name.clone();
        }
        // Its access record, to whoever it lets read the space.
        let access = host
            .group_record(group, uri, &format!("{NS}.access"), "self")
            .map_err(XrpcError::internal)?;
        if let Some(access) = access.filter(|a| Access::of(a).can_read(&roles, me_or)) {
            for key in ["readableBy", "people", "writableBy", "credentialScopes"] {
                if let Some(v) = access.get(key) {
                    space[key] = v.clone();
                }
            }
        }
        spaces.push(space);
    }
    out["spaces"] = json!(spaces);
    // The requests waiting, for one who may admit them.
    if actions.contains("admit") {
        let waiting = host
            .store
            .asking(group, None, 100)
            .map_err(XrpcError::internal)?;
        out["joinRequests"] = waiting
            .iter()
            .map(|r| {
                let mut view = crate::group_join::request_view(r);
                view["actor"] = json!({"did": r.did});
                view
            })
            .collect();
    }
    Ok(Json(out))
}

/// Whether `did` has written their own acceptance in the members space:
/// their repo there holds anything.
fn accepted(host: &Host, members: &str, did: &str) -> anyhow::Result<bool> {
    let empty = atproto::space::LtHash::default().digest();
    let writers = host.store.writers(members, None, u32::MAX)?;
    Ok(writers.iter().any(|w| w.did == did && w.data != empty))
}

/// Every group here: its handle and spaces, and for one whose meta anyone
/// may read, its profile and rules. Unauthenticated.
pub async fn list_groups(State(host): State<Arc<Host>>) -> Answer {
    let mut groups = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = host
            .store
            .accounts(cursor.as_deref(), 100)
            .map_err(XrpcError::internal)?;
        for (did, _) in &page {
            let Ok(handle) = group_of(&host, did) else {
                continue;
            };
            let mut view = base(&host, did, &handle)?;
            if host.deactivation(did)?.is_some() {
                view["deactivated"] = json!(true);
                groups.push(view);
                continue;
            }
            let meta = meta_space(did);
            let access = host
                .group_record(did, &meta, &format!("{NS}.access"), "self")
                .map_err(XrpcError::internal)?;
            if access.is_some_and(|a| Access::of(&a).readable_by.iter().any(|r| r == rbac::PUBLIC))
            {
                let profile = host
                    .group_record(did, &meta, &format!("{NS}.profile"), "self")
                    .map_err(XrpcError::internal)?;
                if let Some(profile) = profile {
                    merge(&mut view, profile_view(&profile));
                }
                view["rules"] = rules(&host, did)?
                    .into_iter()
                    .map(|mut r| {
                        r.as_object_mut().map(|o| o.remove("createdAt"));
                        r
                    })
                    .collect();
            }
            groups.push(view);
        }
        if page.len() < 100 {
            break;
        }
        cursor = page.last().map(|(did, _)| did.clone());
    }
    Ok(Json(json!({ "groups": groups })))
}
