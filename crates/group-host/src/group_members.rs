//! A group's members space as the host reads it, and leaving a group.
//!
//! Roles and memberships are records in the members space, written as the
//! group (`member_writes.rs` says how). The host writes a membership
//! itself only for one who joins. A group never loses its last way to be
//! run: the last member able to configure it may not leave.
//!
//! Someone who stops being a member reads its spaces no more: the
//! credentials they hold are revoked, the apps they signed in as the group
//! end, and they leave the writer set of each of its spaces, the index
//! forgetting what they wrote there and the spaces' syncers told.

use crate::Host;
use crate::group::{did_rkey, members_space};
use crate::group_api::{group_of, signed_in};
use crate::group_config::put_in;
use crate::rbac::{Grant, NS, actions_for, actions_of, roles_of};
use crate::repo::Write;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

fn bad(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

/// A group's members space, its roles' grants, and its records there.
pub(crate) struct Members<'a> {
    pub host: &'a Host,
    pub group: String,
    pub space: String,
    pub grants: Vec<Grant>,
}

impl<'a> Members<'a> {
    pub(crate) fn of(host: &'a Host, group: &str) -> Result<Members<'a>, XrpcError> {
        let space = members_space(group);
        let grants = host.role_grants(group).map_err(XrpcError::internal)?;
        Ok(Members {
            host,
            group: group.into(),
            space,
            grants,
        })
    }

    pub(crate) fn records(&self, collection: &str) -> Result<Vec<(String, Value)>, XrpcError> {
        self.host
            .group_records(&self.group, &self.space, &format!("{NS}.{collection}"))
            .map_err(XrpcError::internal)
    }

    /// `did`'s roles, and whether they are a member at all.
    pub(crate) fn membership(&self, did: &str) -> Result<Option<Vec<String>>, XrpcError> {
        let record = self
            .host
            .group_record(
                &self.group,
                &self.space,
                &format!("{NS}.membership"),
                &did_rkey(did),
            )
            .map_err(XrpcError::internal)?;
        Ok(record.map(|m| roles_of(&m)))
    }

    pub(crate) fn can(&self, roles: &[String], action: &str) -> bool {
        actions_for(&self.grants, roles).contains(action)
    }

    /// The roles every new member gets: those the group marks so, or
    /// `member` in a group that marks none.
    pub(crate) fn default_roles(&self) -> Result<Vec<String>, XrpcError> {
        let marked: Vec<String> = self
            .records("role")?
            .into_iter()
            .filter(|(_, r)| r["default"] == true)
            .map(|(k, _)| k)
            .collect();
        Ok(if marked.is_empty() {
            vec!["member".into()]
        } else {
            marked
        })
    }

    /// Writes `did`'s membership, holding `roles`, and `member` among them
    /// whatever they name: everyone in the group holds it. Its `uri` and
    /// `cid`, as a strong reference names it.
    pub(crate) async fn put_membership(
        &self,
        did: &str,
        roles: &[String],
    ) -> Result<Value, XrpcError> {
        let mut held = roles.to_vec();
        if !held.iter().any(|r| r == "member") {
            held.insert(0, "member".into());
        }
        let membership = json!({
            "$type": format!("{NS}.membership"),
            "roles": held,
            "createdAt": iso(now_millis()),
        });
        let (collection, key) = (format!("{NS}.membership"), did_rkey(did));
        let put = put_in(
            self.host,
            &self.group,
            &self.space,
            &collection,
            &key,
            membership,
        );
        let Json(written) = put.await?;
        Ok(written)
    }

    /// Refuses `who` leaving if nobody else could configure the group.
    fn keeps_a_configurer(&self, who: &str) -> Result<(), XrpcError> {
        let configuring: BTreeSet<String> = self
            .grants
            .iter()
            .filter(|g| actions_of(&g.scopes).contains("group.configure"))
            .map(|g| g.role.clone())
            .collect();
        let others = self.records("membership")?.into_iter().any(|(k, m)| {
            k != did_rkey(who) && roles_of(&m).iter().any(|r| configuring.contains(r))
        });
        if others {
            return Ok(());
        }
        Err(bad(
            "LastConfigurer",
            "nobody would be left who can run this group: give that to someone else first",
        ))
    }
}

/// The group a body names and its members space, once the caller of
/// `lxm` may do `action` in it; and the caller's roles.
pub(crate) async fn acting<'a>(
    host: &'a Host,
    headers: &HeaderMap,
    body: &Value,
    lxm: &str,
    action: Option<&str>,
) -> Result<(Members<'a>, String, Vec<String>), XrpcError> {
    let me = signed_in(host, headers, &format!("{NS}.{lxm}")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(host, group)?;
    let members = Members::of(host, group)?;
    let roles = members.membership(&me)?.unwrap_or_default();
    if let Some(action) = action
        && !members.can(&roles, action)
    {
        return Err(forbidden(format!("missing {action}")));
    }
    Ok((members, me, roles))
}

impl Host {
    /// Ends what `member` holds of `group`: the credentials they hold are
    /// revoked, the apps they signed in as the group end, and their app
    /// passwords go.
    pub(crate) async fn cut_off(&self, group: &str, member: &str) -> Result<(), XrpcError> {
        let revoked = self.revoke_credentials(group, member).await;
        revoked.map_err(XrpcError::internal)?;
        let ended = self.store.revoke_member_grants(group, member);
        ended.map_err(XrpcError::internal)?;
        let dropped = self.store.drop_members_app_passwords(group, member);
        dropped.map_err(XrpcError::internal)?;
        Ok(())
    }

    /// Takes `member` out of the writer set of each of `group`'s spaces:
    /// the index forgets what they wrote there, and the spaces' syncers
    /// are told.
    pub(crate) fn remove_writer_of(&self, group: &str, member: &str) -> Result<(), XrpcError> {
        for space in self.store.spaces_of(group).map_err(XrpcError::internal)? {
            self.remove_writer(&space, member)
                .map_err(XrpcError::internal)?;
        }
        Ok(())
    }

    /// Takes `member` out of `group`: what they hold of it ended first, so
    /// that if that fails they are still in and may ask again; then their
    /// membership, and their place among its spaces' writers.
    async fn put_out(&self, group: &str, member: &str) -> Result<(), XrpcError> {
        self.cut_off(group, member).await?;
        let gone = Write::Delete {
            collection: format!("{NS}.membership"),
            rkey: did_rkey(member),
        };
        let (space, gone) = (members_space(group), [gone]);
        let written = self.space_write(group, &space, &gone);
        written.await?;
        self.remove_writer_of(group, member)
    }
}

/// Leaves the group. Its last member able to configure it may not: nobody
/// could run it again.
pub async fn leave_group(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let (members, me, _) = acting(&host, &headers, &body, "leaveGroup", None).await?;
    if members.membership(&me)?.is_none() {
        return Err(bad("NotAMember", "you are not a member"));
    }
    members.keeps_a_configurer(&me)?;
    host.put_out(&members.group, &me).await?;
    Ok(Json(json!({})))
}
