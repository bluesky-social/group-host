//! Writing as the group, with a token a member approved: records in the
//! group's public repo (`com.atproto.repo.*`) and in its repo in each of
//! its spaces (`com.atproto.space.*`), each as far as the token's scope
//! reaches, and blobs for them.
//!
//! In the group's own meta and members spaces, a token writes only what
//! the host would keep it: in the meta space, the profile, its pictures
//! and the rules, as `group_config` has them, and the labeler, kept at
//! `self` and never deleted; in the members space, roles and memberships,
//! as `member_writes.rs` keeps them, so that nobody gives themselves more
//! than they hold. The group's declaration, in its public repo, follows
//! who may read its meta space. A space's policy, published as
//! `com.atproto.space.policy`, and the group's index of its spaces change
//! only through the space methods (`group_spaces.rs`), never as records.
//!
//! In someone's inbox, a token writes the group's invite to them, as
//! `group_invites` has it.

use crate::Host;
use crate::api::invalid;
use crate::group::{members_space, meta_space};
use crate::group_invites::is_inbox;
use crate::group_spaces::PUBLISHED;
use crate::lexicons::{check_record, validation};
use crate::member_writes::members_writes;
use crate::oauth_resource::{Acting, Refusal};
use crate::oauth_scope::{may_write, meta_writes};
use crate::rbac::{self, LABEL, NS, actions_for, repo_collections_for};
use crate::repo::{Asked, Write, record_cid};
use crate::xrpc::XrpcError;
use atproto::space::SpaceRef;
use axum::Json;
use axum::body::Body;
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

pub(crate) fn missing(scope: impl Into<String>) -> XrpcError {
    XrpcError::new(
        StatusCode::FORBIDDEN,
        "ScopeMissing",
        format!("the token's scope does not hold {}", scope.into()),
    )
}

pub(crate) fn no_longer(collection: &str) -> XrpcError {
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
pub(crate) fn action(write: &Write) -> &'static str {
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

    /// Refuses a write to the group's own meta or members space that the
    /// host would not make itself, and to any space's published policy.
    fn guard(&self, group: &str, space: &str, writes: &[Write]) -> Result<(), XrpcError> {
        let c = |name: &str| format!("{NS}.{name}");
        let label = LABEL.to_string();
        let allowed = if space == meta_space(group) {
            Some([c("profile"), c("rule"), c("labeler"), label].to_vec())
        } else if space == members_space(group) {
            Some([c("role"), c("membership"), label].to_vec())
        } else {
            None
        };
        for write in writes {
            let (collection, rkey) = write.path();
            let ours = allowed
                .as_ref()
                .is_none_or(|a| a.iter().any(|x| x == collection));
            let kept = [PUBLISHED.to_string(), c("space")].contains(&collection.to_string());
            if !ours || kept {
                return Err(forbidden(format!(
                    "{collection} changes through the host's methods, not as a record"
                )));
            }
            if collection == c("labeler") {
                if rkey != "self" {
                    return Err(invalid(format!("{collection} is kept at self")));
                }
                if matches!(write, Write::Delete { .. }) {
                    return Err(invalid(format!("the group keeps its {collection}")));
                }
            }
            if space == meta_space(group) {
                self.face(group, write)?;
            }
        }
        self.beside(group, space, writes)
    }
}

impl Host {
    /// Keeps word in the group's log of each of `writes`, made with
    /// `acting`'s grant, in `space` if one.
    pub(crate) fn wrote(
        &self,
        acting: &Acting,
        space: Option<&str>,
        writes: &[Write],
    ) -> Result<(), XrpcError> {
        let g = &acting.grant;
        for w in writes {
            let (collection, rkey) = w.path();
            let at = space.map(|s| format!(" in {s}")).unwrap_or_default();
            let done = match w {
                Write::Create { .. } => "created",
                Write::Update { .. } => "updated",
                Write::Delete { .. } => "deleted",
            };
            let action = format!("{done} {collection}/{rkey}{at}");
            let recorded = self.record(&g.group, &g.member, Some(&g.client_id), &action);
            recorded.map_err(XrpcError::internal)?;
        }
        Ok(())
    }
}

/// Refuses a record of our own collections, among `writes`, that is not
/// what its lexicon describes.
fn checked(writes: &[Write]) -> Result<(), XrpcError> {
    for write in writes {
        if let Write::Create {
            collection, value, ..
        }
        | Write::Update {
            collection, value, ..
        } = write
        {
            check_record(collection, value)?;
        }
    }
    Ok(())
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
                "validationStatus": validation(collection),
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
    /// group's roles' grants. What a token may do is asked of them at each
    /// use, so a member whose roles shrink gives the app less at once.
    pub(crate) fn standing(
        &self,
        acting: &Acting,
    ) -> Result<(Vec<String>, Vec<rbac::Grant>), XrpcError> {
        let (group, member) = (&acting.grant.group, &acting.grant.member);
        let roles = self.roles(group, member).map_err(XrpcError::internal)?;
        let granted = self.role_grants(group).map_err(XrpcError::internal)?;
        Ok((roles, granted))
    }

    /// Whether the token may make each of `writes` in the group's repo,
    /// and its member's roles still let them.
    fn may_repo(&self, acting: &Acting, writes: &[Write]) -> Result<(), XrpcError> {
        let (roles, grants) = self.standing(acting)?;
        let repo = repo_collections_for(&grants, &roles);
        // A role may publish what it may write as the group in a space, as
        // it may write it there.
        let publishable = self.publishable(&acting.grant.group, &roles)?;
        let configures = actions_for(&grants, &roles).contains("group.configure");
        for w in writes {
            let (collection, _) = w.path();
            if !acting.permissions.repo(collection, action(w)) {
                return Err(missing(format!("repo:{collection}?action={}", action(w))));
            }
            let granted = repo.contains("*") || repo.contains(collection);
            if !granted && !may_write(&publishable, collection, action(w)) {
                return Err(no_longer(collection));
            }
            self.declaration(&acting.grant.group, w)?;
            self.site_write(configures, w)?;
        }
        checked(writes)
    }
}

/// What a body asks of the repo as it stands when the write is made.
fn swaps(body: &Value) -> Result<(Option<&str>, Option<Option<&str>>), XrpcError> {
    let record = match body.get("swapRecord") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(
            v.as_str()
                .ok_or_else(|| invalid("swapRecord is a CID, or null"))?,
        )),
    };
    Ok((body["swapCommit"].as_str(), record))
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
    host.spend_writes(group, std::slice::from_ref(&write))?;
    let (swap_commit, _) = swaps(&body)?;
    let allow = |w: &[Write]| host.may_repo(&acting, w);
    let asked = Asked {
        swap_commit,
        allow: Some(&allow),
        ..Asked::default()
    };
    let made = host.write_asked(group, std::slice::from_ref(&write), &asked)?;
    let (committed, made) = made.expect("a create makes a commit");
    host.wrote(&acting, None, &made)?;
    Ok(Json(json!({
        "uri": repo_uri(group, &collection, &rkey),
        "cid": record_cid(&collection, &body["record"])?.to_string(),
        "commit": commit_view(&committed),
        "validationStatus": validation(&collection),
    })))
}

/// Puts a record, or deletes one, as one write whose create or update is
/// whichever the repo makes it as it is written.
async fn put_or_delete(
    host: &Host,
    nsid: &str,
    headers: &HeaderMap,
    body: &Value,
    delete: bool,
) -> Answer {
    let acting = host.writer(headers, nsid, body)?;
    let group = &acting.grant.group;
    let write = one(body, delete)?;
    host.spend_writes(group, std::slice::from_ref(&write))?;
    let (collection, rkey) = write.path();
    let (swap_commit, swap_record) = swaps(body)?;
    let allow = |w: &[Write]| host.may_repo(&acting, w);
    let asked = Asked {
        swap_commit,
        swap_record,
        put: true,
        allow: Some(&allow),
        ..Asked::default()
    };
    // A put of what is there, or a delete of what is not, makes no commit.
    let committed = host.write_asked(group, std::slice::from_ref(&write), &asked)?;
    let mut out = json!({});
    if let Some((c, made)) = &committed {
        out["commit"] = commit_view(c);
        host.wrote(&acting, None, made)?;
    }
    if !delete {
        out["uri"] = json!(repo_uri(group, collection, rkey));
        out["cid"] = json!(record_cid(collection, &body["record"])?.to_string());
        out["validationStatus"] = json!(validation(collection));
    }
    Ok(Json(out))
}

pub async fn put_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    put_or_delete(&host, "com.atproto.repo.putRecord", &headers, &body, false).await
}

pub async fn delete_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    put_or_delete(
        &host,
        "com.atproto.repo.deleteRecord",
        &headers,
        &body,
        true,
    )
    .await
}

pub async fn apply_writes(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let acting = host.writer(&headers, "com.atproto.repo.applyWrites", &body)?;
    let group = &acting.grant.group;
    let writes = listed(&host, "repo", &body)?;
    host.spend_writes(group, &writes)?;
    let (swap_commit, _) = swaps(&body)?;
    let allow = |w: &[Write]| host.may_repo(&acting, w);
    let asked = Asked {
        swap_commit,
        allow: Some(&allow),
        ..Asked::default()
    };
    let made = host.write_asked(group, &writes, &asked)?;
    let (committed, made) = made.expect("writes that are not puts make a commit");
    host.wrote(&acting, None, &made)?;
    let results: Vec<Value> = writes
        .iter()
        .map(|w| result("repo", repo_uri(group, w.path().0, w.path().1), w))
        .collect::<Result<_, _>>()?;
    Ok(Json(
        json!({"commit": commit_view(&committed), "results": results}),
    ))
}

/// A blob for the group's records, of a type the token's scope takes.
pub async fn upload_blob(State(host): State<Arc<Host>>, headers: HeaderMap, body: Body) -> Answer {
    let acting = host.acting(&headers, "POST", "/xrpc/com.atproto.repo.uploadBlob")?;
    let group = &acting.grant.group;
    host.active(group)?;
    let (roles, grants) = host.standing(&acting)?;
    if !host.writes_anything(group, &roles, &grants)? {
        let no_more = "the member who signed this app in may no longer write anything as the group";
        return Err(forbidden(no_more).into());
    }
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !acting.permissions.blob(mime) {
        return Err(missing(format!("blob:{mime}")).into());
    }
    host.spend(&[crate::limits::BLOBS], group, 1)?;
    let limit = host.config.blob_upload_limit;
    let bytes = crate::xrpc::read_body(body, limit, "BlobTooLarge").await?;
    let blob = host.upload_blob(group, &bytes, mime)?;
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
    pub(crate) fn may_space(
        &self,
        acting: &Acting,
        uri: &str,
        space: &SpaceRef,
        writes: &[Write],
    ) -> Result<(), XrpcError> {
        let group = &acting.grant.group;
        let (roles, grants) = self.standing(acting)?;
        let held = actions_for(&grants, &roles);
        let policy = self.policy(uri).map_err(XrpcError::internal)?;
        let mut writable = policy.map(|p| p.writes(&roles)).unwrap_or_default();
        // The meta space's own records, as its policy and the group's roles
        // grant them.
        if uri == meta_space(group) {
            writable.extend(meta_writes(&held));
        }
        let labels = held.contains("label");
        for w in writes {
            let (collection, _) = w.path();
            if !acting.permissions.space(space, Some(collection), action(w)) {
                return Err(missing(format!(
                    "space:{}?collection={collection}&action={}",
                    space.kind,
                    action(w)
                )));
            }
            let still = match collection {
                LABEL => labels,
                _ if uri == members_space(group) => {
                    may_write(&members_writes(&grants, &roles), collection, action(w))
                }
                _ => may_write(&writable, collection, action(w)),
            };
            if !still {
                return Err(no_longer(collection));
            }
        }
        self.guard(group, uri, writes)?;
        checked(writes)?;
        if uri == members_space(group) {
            self.members_rules(group, &roles, &grants, writes)?;
        }
        Ok(())
    }
}

fn space_uri(space: &str, group: &str, collection: &str, rkey: &str) -> String {
    format!("{space}/{group}/{collection}/{rkey}")
}

impl Host {
    /// Makes `writes` in `space`, at `uri`, as `acting`, as `put` has
    /// them, each as far as its scope and its member's roles reach and as
    /// the host would keep it: whether a commit was made.
    pub(crate) async fn write_as(
        &self,
        acting: &Acting,
        uri: &str,
        space: &SpaceRef,
        writes: &[Write],
        put: bool,
    ) -> Result<bool, XrpcError> {
        let group = &acting.grant.group;
        let members = uri == members_space(group);
        let leaving = match members {
            true => self.leaving(acting, uri, space, writes).await?,
            false => Default::default(),
        };
        let going = self.labels_going(group, uri, writes)?;
        let allow = |w: &[Write]| self.may_space(acting, uri, space, w);
        let asked = Asked {
            put,
            allow: Some(&allow),
            ..Asked::default()
        };
        let made = self.space_write_asked(group, uri, writes, &asked).await?;
        let Some((_, made)) = made else {
            return Ok(false);
        };
        self.wrote(acting, Some(uri), &made)?;
        self.labelled(group, uri, &acting.grant.member, &made, &going)?;
        // In the members space, what a write means is done too.
        if members {
            self.members_written(acting, &made, &leaving).await?;
        }
        Ok(true)
    }
}

/// Makes `writes` in the space a body names, as `put` has them: the
/// space, the group, the writes, and whether a commit was made.
async fn space_writes(
    host: &Host,
    nsid: &str,
    headers: &HeaderMap,
    body: &Value,
    writes: impl FnOnce(&Host) -> Result<Vec<Write>, XrpcError>,
    put: bool,
) -> Result<(String, String, Vec<Write>, bool), Refusal> {
    let acting = host.writer(headers, nsid, body)?;
    let (uri, space) = space_of(body)?;
    let writes = writes(host)?;
    let group = acting.grant.group.clone();
    host.spend_writes(&group, &writes)?;
    // Someone's inbox, where the group writes invites, is theirs.
    if is_inbox(&space) {
        let made = host.write_invites(&acting, &space, writes).await?;
        return Ok((uri, group, made, true));
    }
    let made = host.write_as(&acting, &uri, &space, &writes, put).await?;
    Ok((uri, group, writes, made))
}

/// The one write a put or delete body asks for.
fn one(body: &Value, delete: bool) -> Result<Write, XrpcError> {
    let (collection, rkey) = (
        text(body, "collection")?.to_string(),
        text(body, "rkey")?.to_string(),
    );
    Ok(match delete {
        true => Write::Delete { collection, rkey },
        false => Write::Update {
            collection,
            rkey,
            value: body["record"].clone(),
        },
    })
}

/// A record view of a space write: its URI, CID and validation.
fn space_record_view(
    uri: &str,
    group: &str,
    write: &Write,
    body: &Value,
) -> Result<Value, XrpcError> {
    let (collection, rkey) = write.path();
    Ok(json!({
        "uri": space_uri(uri, group, collection, rkey),
        "cid": record_cid(collection, write.value().unwrap_or(&body["record"]))?.to_string(),
        "validationStatus": validation(collection),
    }))
}

pub async fn space_create_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let rkey = body["rkey"]
        .as_str()
        .map_or_else(|| host.clock.next(), String::from);
    let make = |_: &Host| {
        let collection = text(&body, "collection")?.to_string();
        Ok(vec![Write::Create {
            collection,
            rkey: rkey.clone(),
            value: body["record"].clone(),
        }])
    };
    let nsid = "com.atproto.space.createRecord";
    let (uri, group, writes, _) = space_writes(&host, nsid, &headers, &body, make, false).await?;
    Ok(Json(space_record_view(&uri, &group, &writes[0], &body)?))
}

pub async fn space_put_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let make = |_: &Host| Ok(vec![one(&body, false)?]);
    let nsid = "com.atproto.space.putRecord";
    let (uri, group, writes, _) = space_writes(&host, nsid, &headers, &body, make, true).await?;
    Ok(Json(space_record_view(&uri, &group, &writes[0], &body)?))
}

pub async fn space_delete_record(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let make = |_: &Host| Ok(vec![one(&body, true)?]);
    let nsid = "com.atproto.space.deleteRecord";
    space_writes(&host, nsid, &headers, &body, make, true).await?;
    Ok(Json(json!({})))
}

pub async fn space_apply_writes(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let make = |h: &Host| listed(h, "space", &body);
    let nsid = "com.atproto.space.applyWrites";
    let (uri, group, writes, _) = space_writes(&host, nsid, &headers, &body, make, false).await?;
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
