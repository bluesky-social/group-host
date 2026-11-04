//! Writing as the group, with a token a member approved: records in the
//! group's public repo (`com.atproto.repo.*`) and in its repo in each of
//! its spaces (`com.atproto.space.*`), each as far as the token's scope
//! reaches, and blobs for them.
//!
//! In the group's own meta and members spaces, a token writes only what
//! the configure set is for, and only as the host's methods would: the
//! profile (with a name and a join policy it knows) and the labeler, kept
//! at `self` and never deleted; rules; and roles, one taken away only once
//! nothing grants it. Who holds which role, what each may do, and who may
//! read a space change only through the host's methods, which check that
//! nobody gives themselves more than they hold.
//!
//! As the prototype's `pds/api/repo.ts`, `pds/api/space.ts` and
//! `groupWrites.ts`.

use crate::Host;
use crate::api::invalid;
use crate::group::{join_policy, members_space, meta_space};
use crate::oauth_resource::{Acting, Refusal};
use crate::oauth_scope::{may_write, writes_for};
use crate::rbac::{self, Access, NS, actions_for, grants, repo_collections_for};
use crate::repo::{Write, record_cid};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, Refusal>;

/// The most writes one `applyWrites` makes.
const WRITES_AT_MOST: usize = 200;

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

fn missing(scope: impl Into<String>) -> XrpcError {
    XrpcError::new(
        StatusCode::FORBIDDEN,
        "ScopeMissing",
        format!("the token's scope does not hold {}", scope.into()),
    )
}

fn no_longer(collection: &str) -> XrpcError {
    forbidden(format!(
        "the member who signed this app in may no longer write {collection} as the group"
    ))
}

fn text<'a>(body: &'a Value, key: &str) -> Result<&'a str, XrpcError> {
    body[key]
        .as_str()
        .ok_or_else(|| invalid(format!("{key} is required")))
}

/// What a write does, as a scope names it.
fn action(write: &Write) -> &'static str {
    match write {
        Write::Create { .. } => "create",
        Write::Update { .. } => "update",
        Write::Delete { .. } => "delete",
    }
}

impl Host {
    /// The token on a request to `nsid`, once the repo its body names is
    /// the group it acts as.
    fn writer(&self, headers: &HeaderMap, nsid: &str, body: &Value) -> Result<Acting, Refusal> {
        let acting = self.acting(headers, "POST", &format!("/xrpc/{nsid}"))?;
        let repo = text(body, "repo")?;
        let account = self.store.account(repo).map_err(XrpcError::internal)?;
        if account.is_none_or(|a| a.did != acting.grant.group) {
            return Err(forbidden("a token writes only its own group's repo").into());
        }
        Ok(acting)
    }

    /// The write a body asks for at `collection`/`rkey`: a create where
    /// there is no record, else an update; `None` for a put of what is
    /// there already.
    fn put(
        &self,
        current: Option<atproto::Cid>,
        collection: &str,
        rkey: &str,
        value: &Value,
    ) -> Result<Option<Write>, XrpcError> {
        let (collection, rkey, value) = (collection.to_string(), rkey.to_string(), value.clone());
        Ok(match current {
            None => Some(Write::Create {
                collection,
                rkey,
                value,
            }),
            Some(cid) if cid == record_cid(&collection, &value)? => None,
            Some(_) => Some(Write::Update {
                collection,
                rkey,
                value,
            }),
        })
    }

    /// Refuses a write to the group's own meta or members space that the
    /// host would not make itself, and to any space's access record.
    fn guard(&self, group: &str, space: &str, writes: &[Write]) -> Result<(), XrpcError> {
        let c = |name: &str| format!("{NS}.{name}");
        let allowed = if space == meta_space(group) {
            Some([c("profile"), c("rule"), c("labeler")].to_vec())
        } else if space == members_space(group) {
            Some([c("role")].to_vec())
        } else {
            None
        };
        for write in writes {
            let (collection, rkey) = write.path();
            let ours = allowed
                .as_ref()
                .is_none_or(|a| a.iter().any(|x| x == collection));
            if !ours || collection == c("access") {
                return Err(forbidden(format!(
                    "{collection} changes through the host's methods, not as a record"
                )));
            }
            if [c("profile"), c("labeler")].iter().any(|x| x == collection) {
                if rkey != "self" {
                    return Err(invalid(format!("{collection} is kept at self")));
                }
                if matches!(write, Write::Delete { .. }) {
                    return Err(invalid(format!("the group keeps its {collection}")));
                }
            }
            if let Write::Create { value, .. } | Write::Update { value, .. } = write
                && collection == c("profile")
            {
                let named = value["displayName"]
                    .as_str()
                    .is_some_and(|n| !n.trim().is_empty());
                if !named {
                    return Err(invalid("displayName is required"));
                }
                join_policy(value["joinPolicy"].as_str())?;
            }
            if matches!(write, Write::Delete { .. }) && collection == c("role") {
                let permissions = self.group_record(group, space, &c("permissions"), "self");
                let permissions = permissions
                    .map_err(XrpcError::internal)?
                    .unwrap_or_default();
                if grants(&permissions).iter().any(|g| g.role == rkey) {
                    return Err(XrpcError::new(
                        StatusCode::BAD_REQUEST,
                        "RoleInUse",
                        format!("role {rkey} still has a grant; deleteRole takes both away"),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A write's result, as `applyWrites` lists them.
fn result(kind: &str, uri: String, write: &Write) -> Result<Value, XrpcError> {
    let (collection, _) = write.path();
    Ok(match write {
        Write::Delete { .. } => {
            json!({"$type": format!("com.atproto.{kind}.applyWrites#deleteResult")})
        }
        Write::Create { value, .. } | Write::Update { value, .. } => {
            let which = if matches!(write, Write::Create { .. }) {
                "createResult"
            } else {
                "updateResult"
            };
            json!({
                "$type": format!("com.atproto.{kind}.applyWrites#{which}"),
                "uri": uri,
                "cid": record_cid(collection, value)?.to_string(),
                "validationStatus": "unknown",
            })
        }
    })
}

/// The writes an `applyWrites` body lists.
fn listed(host: &Host, kind: &str, body: &Value) -> Result<Vec<Write>, XrpcError> {
    let writes = body["writes"]
        .as_array()
        .ok_or_else(|| invalid("writes is required"))?;
    if writes.len() > WRITES_AT_MOST {
        return Err(invalid(format!("at most {WRITES_AT_MOST} writes at once")));
    }
    let mut out = Vec::new();
    for w in writes {
        let collection = text(w, "collection")?.to_string();
        let rkey = || w["rkey"].as_str().map(String::from);
        let value = || w["value"].clone();
        let which = w["$type"].as_str().unwrap_or_default();
        let ns = format!("com.atproto.{kind}.applyWrites#");
        out.push(match which.strip_prefix(&ns) {
            Some("create") => Write::Create {
                collection,
                rkey: rkey().unwrap_or_else(|| host.clock.next()),
                value: value(),
            },
            Some("update") => Write::Update {
                collection,
                rkey: rkey().ok_or_else(|| invalid("an update names its rkey"))?,
                value: value(),
            },
            Some("delete") => Write::Delete {
                collection,
                rkey: rkey().ok_or_else(|| invalid("a delete names its rkey"))?,
            },
            _ => return Err(invalid(format!("not a write: {which}"))),
        });
    }
    Ok(out)
}

// ---- the group's public repo ----

fn commit_view(c: &crate::repo::Committed) -> Value {
    json!({"cid": c.cid.to_string(), "rev": c.rev})
}

impl Host {
    /// The roles the member who approved a grant holds now, and the
    /// group's grants. What a token may do is asked of them at each use,
    /// so a member whose roles shrink gives the app less at once.
    fn standing(&self, acting: &Acting) -> Result<(Vec<String>, Vec<rbac::Grant>), XrpcError> {
        let (group, member) = (&acting.grant.group, &acting.grant.member);
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let permissions = self.group_record(
            group,
            &members_space(group),
            &format!("{NS}.permissions"),
            "self",
        );
        let permissions = permissions
            .map_err(XrpcError::internal)?
            .unwrap_or_default();
        Ok((roles, grants(&permissions)))
    }

    /// Whether the token may make each of `writes` in the group's repo,
    /// and its member's roles still let them.
    fn may_repo(&self, acting: &Acting, writes: &[Write]) -> Result<(), XrpcError> {
        let (roles, grants) = self.standing(acting)?;
        let repo = repo_collections_for(&grants, &roles);
        for w in writes {
            let (collection, _) = w.path();
            if !acting.permissions.repo(collection, action(w)) {
                return Err(missing(format!("repo:{collection}?action={}", action(w))));
            }
            if !repo.contains("*") && !repo.contains(collection) {
                return Err(no_longer(collection));
            }
        }
        Ok(())
    }

    fn repo_record(
        &self,
        group: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<Option<atproto::Cid>, XrpcError> {
        let found = self
            .store
            .record(group, collection, rkey)
            .map_err(XrpcError::internal)?;
        Ok(found.map(|r| r.cid))
    }
}

fn invalid_swap(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidSwap", message)
}

impl Host {
    /// Refuses a write whose `swapCommit` is not the repo's head.
    fn swap_commit(&self, group: &str, body: &Value) -> Result<(), XrpcError> {
        let Some(swap) = body["swapCommit"].as_str() else {
            return Ok(());
        };
        let head = self.store.head(group).map_err(XrpcError::internal)?;
        if head.is_none_or(|h| h.cid.to_string() != swap) {
            return Err(invalid_swap(format!("the repo is not at {swap}")));
        }
        Ok(())
    }
}

/// Refuses a write whose `swapRecord` is not the record there now: a CID,
/// or null for none.
fn swap_record(body: &Value, current: Option<atproto::Cid>) -> Result<(), XrpcError> {
    let Some(swap) = body.get("swapRecord") else {
        return Ok(());
    };
    if swap.as_str() != current.map(|c| c.to_string()).as_deref() {
        return Err(invalid_swap("the record is not the one swapped"));
    }
    Ok(())
}

/// `at://` of a record in the group's repo.
fn repo_uri(group: &str, collection: &str, rkey: &str) -> String {
    format!("at://{group}/{collection}/{rkey}")
}

pub async fn create_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.repo.createRecord", &body)?;
    let group = &acting.grant.group;
    let collection = text(&body, "collection")?.to_string();
    let rkey = body["rkey"]
        .as_str()
        .map_or_else(|| host.clock.next(), String::from);
    let write = Write::Create {
        collection: collection.clone(),
        rkey: rkey.clone(),
        value: body["record"].clone(),
    };
    host.may_repo(&acting, std::slice::from_ref(&write))?;
    host.swap_commit(group, &body)?;
    let committed = host.write(group, std::slice::from_ref(&write))?;
    Ok(Json(json!({
        "uri": repo_uri(group, &collection, &rkey),
        "cid": record_cid(&collection, &body["record"])?.to_string(),
        "commit": commit_view(&committed),
        "validationStatus": "unknown",
    })))
}

pub async fn put_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.repo.putRecord", &body)?;
    let group = &acting.grant.group;
    let (collection, rkey) = (text(&body, "collection")?, text(&body, "rkey")?);
    let current = host.repo_record(group, collection, rkey)?;
    swap_record(&body, current)?;
    host.swap_commit(group, &body)?;
    let mut out = json!({
        "uri": repo_uri(group, collection, rkey),
        "cid": record_cid(collection, &body["record"])?.to_string(),
        "validationStatus": "unknown",
    });
    // A put of what is there already changes nothing, and makes no commit.
    if let Some(write) = host.put(current, collection, rkey, &body["record"])? {
        host.may_repo(&acting, std::slice::from_ref(&write))?;
        out["commit"] = commit_view(&host.write(group, &[write])?);
    }
    Ok(Json(out))
}

pub async fn delete_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.repo.deleteRecord", &body)?;
    let group = &acting.grant.group;
    let (collection, rkey) = (
        text(&body, "collection")?.to_string(),
        text(&body, "rkey")?.to_string(),
    );
    let write = Write::Delete {
        collection: collection.clone(),
        rkey: rkey.clone(),
    };
    host.may_repo(&acting, std::slice::from_ref(&write))?;
    let current = host.repo_record(group, &collection, &rkey)?;
    swap_record(&body, current)?;
    host.swap_commit(group, &body)?;
    // Gone already is gone, as a PDS has it.
    if current.is_none() {
        return Ok(Json(json!({})));
    }
    let committed = host.write(group, &[write])?;
    Ok(Json(json!({"commit": commit_view(&committed)})))
}

pub async fn apply_writes(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.repo.applyWrites", &body)?;
    let group = &acting.grant.group;
    let writes = listed(&host, "repo", &body)?;
    host.may_repo(&acting, &writes)?;
    host.swap_commit(group, &body)?;
    let committed = host.write(group, &writes)?;
    let results: Vec<Value> = writes
        .iter()
        .map(|w| result("repo", repo_uri(group, w.path().0, w.path().1), w))
        .collect::<Result<_, _>>()?;
    Ok(Json(
        json!({"commit": commit_view(&committed), "results": results}),
    ))
}

/// A blob for the group's records, of a type the token's scope takes.
pub async fn upload_blob(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Answer {
    let acting = host.acting(&headers, "POST", "/xrpc/com.atproto.repo.uploadBlob")?;
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !acting.permissions.blob(mime) {
        return Err(missing(format!("blob:{mime}")).into());
    }
    let blob = host.upload_blob(&acting.grant.group, &bytes, mime)?;
    Ok(Json(json!({ "blob": blob })))
}

// ---- the group's repos in its spaces ----

/// The space a body names, as a scope is asked of it.
fn space_of(body: &Value) -> Result<(String, SpaceRef), XrpcError> {
    let uri = text(body, "space")?;
    let space = SpaceRef::parse(uri).map_err(|e| invalid(e.to_string()))?;
    Ok((uri.to_string(), space))
}

impl Host {
    /// Whether the token may make each of `writes` in `space`, its
    /// member's roles still let them, and the host would make them.
    fn may_space(
        &self,
        acting: &Acting,
        uri: &str,
        space: &SpaceRef,
        writes: &[Write],
    ) -> Result<(), XrpcError> {
        let group = &acting.grant.group;
        let (roles, grants) = self.standing(acting)?;
        let own = uri == meta_space(group) || uri == members_space(group);
        let configures = actions_for(&grants, &roles).contains("group.configure");
        let access = self.group_record(group, uri, &format!("{NS}.access"), "self");
        let access = access.map_err(XrpcError::internal)?.unwrap_or_default();
        let rules = Access::of(&access);
        let everything = repo_collections_for(&grants, &roles).contains("*");
        let writable = writes_for(&access, &roles);
        for w in writes {
            let (collection, _) = w.path();
            if !acting.permissions.space(space, Some(collection), action(w)) {
                return Err(missing(format!(
                    "space:{}?collection={collection}&action={}",
                    space.kind,
                    action(w)
                )));
            }
            let still = match &rules.people {
                _ if own => configures,
                Some(_) => everything && rules.can_read(&roles, &acting.grant.member),
                None => may_write(&writable, collection, action(w)),
            };
            if !still {
                return Err(no_longer(collection));
            }
        }
        self.guard(group, uri, writes)
    }

    fn space_record_cid(
        &self,
        group: &str,
        space: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<Option<atproto::Cid>, XrpcError> {
        let found = self
            .store
            .space_record(group, space, collection, rkey)
            .map_err(XrpcError::internal)?;
        Ok(found.map(|r| r.cid))
    }
}

fn space_uri(space: &str, group: &str, collection: &str, rkey: &str) -> String {
    format!("{space}/{group}/{collection}/{rkey}")
}

pub async fn space_create_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.space.createRecord", &body)?;
    let (uri, space) = space_of(&body)?;
    let group = acting.grant.group.clone();
    let collection = text(&body, "collection")?.to_string();
    let rkey = body["rkey"]
        .as_str()
        .map_or_else(|| host.clock.next(), String::from);
    let write = Write::Create {
        collection: collection.clone(),
        rkey: rkey.clone(),
        value: body["record"].clone(),
    };
    host.may_space(&acting, &uri, &space, std::slice::from_ref(&write))?;
    host.space_write(&group, &uri, &[write]).await?;
    Ok(Json(json!({
        "uri": space_uri(&uri, &group, &collection, &rkey),
        "cid": record_cid(&collection, &body["record"])?.to_string(),
        "validationStatus": "unknown",
    })))
}

pub async fn space_put_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.space.putRecord", &body)?;
    let (uri, space) = space_of(&body)?;
    let group = acting.grant.group.clone();
    let (collection, rkey) = (text(&body, "collection")?, text(&body, "rkey")?);
    let current = host.space_record_cid(&group, &uri, collection, rkey)?;
    if let Some(write) = host.put(current, collection, rkey, &body["record"])? {
        host.may_space(&acting, &uri, &space, std::slice::from_ref(&write))?;
        host.space_write(&group, &uri, &[write]).await?;
    }
    Ok(Json(json!({
        "uri": space_uri(&uri, &group, collection, rkey),
        "cid": record_cid(collection, &body["record"])?.to_string(),
        "validationStatus": "unknown",
    })))
}

pub async fn space_delete_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.space.deleteRecord", &body)?;
    let (uri, space) = space_of(&body)?;
    let group = acting.grant.group.clone();
    let (collection, rkey) = (
        text(&body, "collection")?.to_string(),
        text(&body, "rkey")?.to_string(),
    );
    let write = Write::Delete {
        collection: collection.clone(),
        rkey: rkey.clone(),
    };
    host.may_space(&acting, &uri, &space, std::slice::from_ref(&write))?;
    if host
        .space_record_cid(&group, &uri, &collection, &rkey)?
        .is_some()
    {
        host.space_write(&group, &uri, &[write]).await?;
    }
    Ok(Json(json!({})))
}

pub async fn space_apply_writes(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.space.applyWrites", &body)?;
    let (uri, space) = space_of(&body)?;
    let group = acting.grant.group.clone();
    let writes = listed(&host, "space", &body)?;
    host.may_space(&acting, &uri, &space, &writes)?;
    host.space_write(&group, &uri, &writes).await?;
    let results: Vec<Value> = writes
        .iter()
        .map(|w| result("space", space_uri(&uri, &group, w.path().0, w.path().1), w))
        .collect::<Result<_, _>>()?;
    Ok(Json(json!({ "results": results })))
}

/// Who the token is for: the group, and that it is switched on.
pub async fn get_session(State(host): State<Arc<Host>>, headers: HeaderMap) -> Answer {
    let acting = host.acting(&headers, "GET", "/xrpc/com.atproto.server.getSession")?;
    let group = &acting.grant.group;
    let account = host.store.account(group).map_err(XrpcError::internal)?;
    let handle = account.map(|a| a.handle).unwrap_or_default();
    let active = host.deactivation(group)?.is_none();
    let mut out = json!({"did": group, "handle": handle, "active": active});
    if !active {
        out["status"] = json!("deactivated");
    }
    Ok(Json(out))
}
