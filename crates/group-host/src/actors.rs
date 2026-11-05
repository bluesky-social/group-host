//! People, as the console shows them: who is asking (`getViewer`, as
//! knotless's), a person (`getActor`), and the groups someone is in
//! (`getActorGroups`).

use crate::Host;
use crate::api::{Params, page, param};
use crate::group::{members_space, meta_space};
use crate::group_api::{caller, group_of, signed_in};
use crate::rbac::{Access, NS};
use crate::store::Store;
use crate::xrpc::XrpcError;
use atproto::did;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// Who is asking, by the console's cookie or service auth: nothing, for
/// no one.
pub async fn get_viewer(State(host): State<Arc<Host>>, headers: HeaderMap) -> Answer {
    let Some(me) = caller(&host, &headers, &format!("{NS}.getViewer")).await? else {
        return Ok(Json(json!({})));
    };
    let mut actor = json!({"did": me, "canCreateGroup": host.may_found(&me)});
    let doc = host.identity.document(&me).await.ok();
    if let Some(handle) = doc.as_ref().and_then(did::handle) {
        actor["handle"] = json!(handle);
    }
    Ok(Json(json!({ "actor": actor })))
}

impl Host {
    /// A person as a list names them: their DID, and the handle their DID
    /// document names, if it can be read.
    pub(crate) async fn actor_view(&self, did: &str) -> Value {
        let doc = self.identity.document(did).await.ok();
        match doc.as_ref().and_then(did::handle) {
            Some(handle) => json!({"did": did, "handle": handle}),
            None => json!({"did": did}),
        }
    }

    /// A group as a list of someone's shows it, to `viewer`, a member: its
    /// name and picture as they may read them, and how many are in it.
    fn group_view_basic(&self, group: &str, viewer: &str) -> Answer {
        let handle = group_of(self, group)?;
        let mut view = json!({"did": group, "handle": handle});
        if let Some(address) = self.store.address(group).map_err(XrpcError::internal)? {
            view["address"] = json!(address);
        }
        let roles = self.roles(group, viewer).map_err(XrpcError::internal)?;
        let meta = meta_space(group);
        let access = self.group_record(group, &meta, &format!("{NS}.access"), "self");
        let access = access.map_err(XrpcError::internal)?.unwrap_or_default();
        if Access::of(&access, &meta).can_read(&roles, viewer) {
            let profile = self.group_record(group, &meta, &format!("{NS}.profile"), "self");
            let profile = profile.map_err(XrpcError::internal)?.unwrap_or_default();
            if let Some(name) = profile.get("displayName") {
                view["displayName"] = name.clone();
            }
            if let Some(cid) = profile["avatar"]["ref"]["$link"].as_str() {
                view["avatar"] = json!(cid);
            }
        }
        let members = self.group_records(group, &members_space(group), &format!("{NS}.membership"));
        view["memberCount"] = json!(members.map_err(XrpcError::internal)?.len());
        Ok(Json(view))
    }
}

impl Store {
    /// Whether `did` is asking to join any group here that is switched on.
    pub fn asking_anywhere(&self, did: &str) -> anyhow::Result<bool> {
        let sql = "SELECT 1 FROM join_request j WHERE j.did = ?1
                   AND NOT EXISTS (SELECT 1 FROM deactivation d WHERE d.did = j.grp) LIMIT 1";
        let found = self.conn().query_row(sql, [did], |_| Ok(()));
        Ok(rusqlite::OptionalExtension::optional(found)?.is_some())
    }
}

/// A person, as a view names them, for someone signed in: themselves, or
/// anyone the host shows, a member of one of its groups or someone asking
/// to join one; and to themselves, whether the host starts groups for them.
pub async fn get_actor(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.getActor")).await?;
    let actor = param(&p, "actor")?;
    let shown = actor == me
        || !host.groups_of(actor)?.is_empty()
        || host
            .store
            .asking_anywhere(actor)
            .map_err(XrpcError::internal)?;
    if !shown {
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "ActorNotFound",
            format!("no one here by {actor}"),
        ));
    }
    let mut view = host.actor_view(actor).await;
    if actor == me {
        view["canCreateGroup"] = json!(host.may_found(&me));
    }
    Ok(Json(view))
}

/// The groups here someone is in: every one of the caller's own, and of
/// anyone else's only those the caller is in too, a group's members being
/// its members' alone to know. By DID, a page at a time.
pub async fn get_actor_groups(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.getActorGroups")).await?;
    let asked = param(&p, "actor")?.trim_start_matches('@').to_lowercase();
    let actor = match asked.starts_with("did:") {
        true => Some(asked.clone()),
        false => {
            host.identity
                .resolve_handle(&asked, &host.config.handle_resolvers)
                .await
        }
    };
    let not_found = || XrpcError::new(StatusCode::BAD_REQUEST, "ActorNotFound", "no such actor");
    let actor = actor.ok_or_else(not_found)?;
    let limit = page(&p, 50, 100)? as usize;
    let after = p.get("cursor").map(String::as_str).unwrap_or_default();
    let mine = host.groups_of(&me)?;
    let mut theirs: Vec<String> = host
        .groups_of(&actor)?
        .into_iter()
        .filter(|g| mine.contains(g) && g.as_str() > after)
        .collect();
    theirs.sort();
    let more = theirs.len() > limit;
    theirs.truncate(limit);
    let mut groups = Vec::new();
    for group in &theirs {
        groups.push(host.group_view_basic(group, &me)?.0);
    }
    let mut out = json!({ "groups": groups });
    if more {
        out["cursor"] = json!(theirs.last());
    }
    Ok(Json(out))
}
