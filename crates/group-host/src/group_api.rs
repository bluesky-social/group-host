//! The `fyi.opensocial.*` methods that make and read groups: starting
//! one, a group as its viewer may see it, and every group here.
//!
//! A caller is who their service auth says, for this host (`SERVICE_DID`)
//! and the method: their own PDS signs it for them.

use crate::Host;
use crate::api::{Params, page, param};
use crate::group::{Made, NewGroup, members_space, meta_space, rkey_did};
use crate::member_writes::{expanded, held};
use crate::rbac::{
    ACTIONS, Grant, NS, actions_for, actions_of, label_writes_in, listed, repo_collections_for,
    roles_of, scope_of,
};
use crate::scopes::covers;
use crate::space_api::{service_caller, unauthorized};
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// Whether a token for `aud` is for this host: its DID, or one of its
/// services, as an OAuth app's rpc scope names it.
pub(crate) fn for_host(host: &Host, aud: &str) -> bool {
    let ours = &host.config.service_did;
    aud == ours
        || aud
            .strip_prefix(ours.as_str())
            .is_some_and(|f| f.starts_with('#'))
}

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
    if !for_host(host, &claims.aud) {
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
        handle: text("handle").expect("its lexicon requires it"),
        founder,
        stewards: strings(body, "stewards", "did").unwrap_or_default(),
        display_name: text("displayName").expect("its lexicon requires it"),
        description: text("description"),
        join_policy: text("joinPolicy"),
        meta_public_read: body["metaPublicRead"].as_bool(),
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

/// Puts in `view` what a group's profile shows of it.
fn show_profile(view: &mut Value, profile: &Value) {
    for key in [
        "displayName",
        "description",
        "joinPolicy",
        "url",
        "links",
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
    let mut view = json!({
        "did": did,
        "handle": handle,
        "address": address,
        "meta": meta_space(did),
        "members": members_space(did),
    });
    let domain = host.store.site_domain(did).map_err(XrpcError::internal)?;
    if let Some(domain) = domain {
        view["siteDomain"] = json!(domain);
    }
    Ok(view)
}

fn objects(field: &str, items: impl IntoIterator<Item = impl Into<String>>) -> Value {
    items
        .into_iter()
        .map(|i| json!({ field: i.into() }))
        .collect()
}

/// The scopes a role's editor offers that holders of `roles` hold enough
/// of to give, as a role write asks: each action's, then the group's
/// permission sets but `authManageGroup`, which is `group.configure`'s.
fn grantable(grants: &[Grant], roles: &[String], group: &str) -> Vec<Value> {
    let held = held(grants, roles);
    let sets = ["authManageProfile", "authModerate"].map(|s| format!("include:{NS}.{s}"));
    let offered = ACTIONS.iter().filter_map(|a| scope_of(a)).chain(sets);
    offered
        .filter(|s| expanded(s).iter().all(|x| covers(&held, x, group)))
        .map(|s| json!({ "scope": s }))
        .collect()
}

/// A group as someone not signed in sees it: what anyone can learn of it
/// already, its DID and handle.
fn signed_out(did: &str, handle: String) -> Value {
    json!({"did": did, "handle": handle})
}

/// One group in full, as the caller may see it: what its meta space
/// holds for whoever may read it, the caller's own standing, and for a
/// member, how many are in it, its roles and spaces. To no one signed in,
/// only its DID and handle, whatever its meta space's policy says.
pub async fn get_group(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    let handle = group_of(&host, group)?;
    let me = caller(&host, &headers, &format!("{NS}.getGroup")).await?;
    let Some(me) = me else {
        return Ok(Json(json!({ "group": signed_out(group, handle) })));
    };
    // Switched off: nothing in it is read, by members either. Still
    // answered, telling a steward of when it went off so, to find the
    // switch.
    if let Some(off) = host.deactivation(group)? {
        let mut view = base(&host, group, &handle)?;
        view["deactivated"] = json!(true);
        let steward = off.stewards.contains(&me);
        let viewer = json!({"member": false, "roles": [], "actions": [], "steward": steward});
        view["viewer"] = viewer;
        return Ok(Json(json!({ "group": view })));
    }
    let record = |space: &str, collection: &str, rkey: &str| {
        host.group_record(group, space, &format!("{NS}.{collection}"), rkey)
            .map_err(XrpcError::internal)
    };
    let (meta, members) = (meta_space(group), members_space(group));
    let roles = host.roles(group, &me).map_err(XrpcError::internal)?;
    let reads = |space: &str| host.reads_space(space, &roles);
    let reads_meta = reads(&meta)?;
    let reads_members = reads(&members)?;
    let granted = host.role_grants(group).map_err(XrpcError::internal)?;
    let actions = actions_for(&granted, &roles);

    let mut view = base(&host, group, &handle)?;
    if reads_meta {
        if let Some(profile) = record(&meta, "profile", "self")? {
            show_profile(&mut view, &profile);
        }
        view["rules"] = json!(rules(&host, group)?);
        let labeler = record(&meta, "labeler", "self")?.unwrap_or_default();
        let labels = &labeler["policies"]["labelValueDefinitions"];
        view["labels"] = if labels.is_array() {
            labels.clone()
        } else {
            json!([])
        };
    }
    let member = !roles.is_empty();
    let mut viewer = json!({
        "member": member,
        "roles": objects("role", roles.clone()),
        "actions": actions,
    });
    if member {
        viewer["accepted"] = json!(accepted(&host, &members, &me).map_err(XrpcError::internal)?);
        viewer["grantable"] = json!(grantable(&granted, &roles, group));
        viewer["labelSpaces"] = json!(label_spaces(&host, group, &roles, &granted)?);
    }
    // Their own request, while it waits.
    let asked = host.store.asked(group, &me).map_err(XrpcError::internal)?;
    if let Some(request) = asked.filter(|_| !member) {
        viewer["request"] = crate::group_join::request_view(&request);
    }
    view["viewer"] = viewer;
    if !reads_members {
        return Ok(Json(json!({ "group": view })));
    }
    let records = |collection: &str| {
        host.group_records(group, &members, &format!("{NS}.{collection}"))
            .map_err(XrpcError::internal)
    };
    view["memberCount"] = json!(records("membership")?.len());
    let mut out = json!({ "group": view });
    out["roles"] = records("role")?
        .into_iter()
        .map(|(key, r)| {
            let mut role = json!({"key": key});
            for (from, to) in [
                ("displayName", "name"),
                ("description", "description"),
                ("bot", "bot"),
                ("default", "default"),
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
                "scopes": g.scopes.iter().map(|s| json!({
                    "scope": s,
                    "actions": actions_of(std::slice::from_ref(s)),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut spaces = Vec::new();
    // Those in its index, in its order, then those left out of it to
    // whoever reads them; each with its policy, to whoever reads the space.
    let mut uris: Vec<String> = records("space")?
        .into_iter()
        .filter_map(|(_, e)| e["space"].as_str().map(String::from))
        .collect();
    let unlisted = host.store.spaces_of(group).map_err(XrpcError::internal)?;
    let unlisted: Vec<String> = unlisted.into_iter().filter(|u| !uris.contains(u)).collect();
    uris.extend(unlisted);
    for uri in uris {
        let kept = host.hosted_space(&uri)?;
        let readable = reads(&uri)?;
        if kept.policy["unlisted"] == true && !readable {
            continue;
        }
        let kind = uri.split('/').nth(4).unwrap_or_default();
        let mut space = json!({"uri": uri, "spaceType": kind});
        if readable {
            space["policy"] = kept.policy;
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
        let actors = waiting.iter().map(|r| host.actor_view(&r.did));
        let actors = futures::future::join_all(actors).await;
        out["joinRequests"] = waiting
            .iter()
            .zip(actors)
            .map(|(r, actor)| {
                let mut view = crate::group_join::request_view(r);
                view["actor"] = actor;
                view
            })
            .collect();
    }
    Ok(Json(out))
}

/// A group's members, each with their roles, to one who may read its
/// members space: a page at a time, in the order of their DIDs (the keys
/// of their memberships), those after `cursor`; with `q`, only those
/// whose handle or DID holds it, in any case. The cursor is there while
/// more are.
pub async fn list_members(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    group_of(&host, group)?;
    let me = signed_in(&host, &headers, &format!("{NS}.listMembers")).await?;
    let members = members_space(group);
    let roles = host.roles(group, &me).map_err(XrpcError::internal)?;
    if !host.reads_space(&members, &roles)? {
        let closed = "only those who may read its members space see them";
        return Err(XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", closed));
    }
    let limit = page(&p, 50, 100)? as usize;
    let after = p.get("cursor").cloned().unwrap_or_default();
    let q = p
        .get("q")
        .map(|q| q.trim().to_lowercase())
        .unwrap_or_default();
    let memberships = host
        .group_records(group, &members, &format!("{NS}.membership"))
        .map_err(XrpcError::internal)?;
    let left: Vec<(String, Value)> = memberships
        .into_iter()
        .filter(|(rkey, _)| *rkey > after)
        .collect();
    // Named by their documents a page and one more at a time, each page
    // read at once rather than in turn, until a page and one are found.
    let mut found: Vec<(String, Value)> = Vec::new();
    for some in left.chunks(limit + 1) {
        let dids: Vec<String> = some.iter().map(|(k, _)| rkey_did(k)).collect();
        let actors = dids.iter().map(|did| host.actor_view(did));
        let actors = futures::future::join_all(actors).await;
        for ((rkey, m), actor) in some.iter().zip(actors) {
            let named = |at: &str| actor[at].as_str().unwrap_or_default().to_lowercase();
            if named("did").contains(&q) || named("handle").contains(&q) {
                let view = json!({"actor": actor, "roles": objects("role", roles_of(m))});
                found.push((rkey.clone(), view));
            }
        }
        if found.len() > limit {
            break;
        }
    }
    let more = found.len() > limit;
    found.truncate(limit);
    let mut out = json!({ "members": found.iter().map(|(_, v)| v).collect::<Vec<_>>() });
    if let Some((last, _)) = found.last().filter(|_| more) {
        out["cursor"] = json!(last);
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

/// The spaces of `group`'s where a holder of `roles` may write a label as
/// it, by their roles' scopes across the group or the space's policy.
fn label_spaces(
    host: &Host,
    group: &str,
    roles: &[String],
    grants: &[Grant],
) -> Result<Vec<Value>, XrpcError> {
    let spaces = host.store.spaces_of(group).map_err(XrpcError::internal)?;
    let mut out = Vec::new();
    for uri in spaces {
        let policy = host.policy(&uri).map_err(XrpcError::internal)?;
        let policy = policy.expect("a space the group has");
        if label_writes_in(grants, roles, &policy).contains("create") {
            out.push(json!({ "space": uri }));
        }
    }
    Ok(out)
}
