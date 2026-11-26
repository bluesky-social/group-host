//! Groups: an account here with two spaces of its own, `meta` (its about
//! record and config, for whoever it lets read them) and `members` (its
//! roles, each with what it may do, its memberships, rules and labeler,
//! and the index of its spaces, for its members). Everything a group is
//! lives in those records, written by the group; who may read or write
//! each space is its policy, against the reader's roles.

use crate::Host;
use crate::rbac::{EVERY_ROLE, Grant, NS, POLICY, grants, roles_of};
use crate::repo::{Write, invalid};
use crate::store::Record;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::drisl;
use axum::http::StatusCode;
use serde_json::{Value, json};

/// A group's meta space, at `did`.
pub fn meta_space(did: &str) -> String {
    format!("at://{did}/space/{NS}.meta/self")
}

/// A group's members space, at `did`.
pub fn members_space(did: &str) -> String {
    format!("at://{did}/space/{NS}.members/self")
}

/// A DID as a record key: a record key holds no `%`, so a did:web's
/// encoded port is written plain.
pub fn did_rkey(did: &str) -> String {
    did.replace("%3A", ":")
}

/// The DID a record key made by [`did_rkey`] is of: a did:web whose host
/// is followed by a port, all digits, has it encoded again.
pub fn rkey_did(rkey: &str) -> String {
    let Some((host, after)) = rkey
        .strip_prefix("did:web:")
        .and_then(|r| r.split_once(':'))
    else {
        return rkey.to_string();
    };
    let (port, path) = after.split_at(after.find(':').unwrap_or(after.len()));
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return rkey.to_string();
    }
    format!("did:web:{host}%3A{port}{path}")
}

/// The report reasons the labeler every group starts with takes: each of
/// those the proposal names.
pub const REASONS: [&str; 6] = [
    "org.atgrp.mod.defs#reasonSpam",
    "org.atgrp.mod.defs#reasonViolation",
    "org.atgrp.mod.defs#reasonMisleading",
    "org.atgrp.mod.defs#reasonNsfw",
    "org.atgrp.mod.defs#reasonRude",
    "org.atgrp.mod.defs#reasonOther",
];

/// What the labeler every group starts with says: the labels the host
/// acts on, and reports about accounts and records, of every reason.
fn labeler() -> Value {
    json!({
        "$type": format!("{NS}.labeler"),
        "policies": {"labelValues": ["!hide", "!takedown"]},
        "reasonTypes": REASONS,
        "subjectTypes": ["account", "record"],
    })
}

/// How a group lets people in, as its config says it (`joinPolicy`):
/// `open`, `approval` or `invite`; `approval` unless it says.
pub fn join_policy(asked: Option<&str>) -> Result<&'static str, XrpcError> {
    match asked {
        None | Some("approval") => Ok("approval"),
        Some("open") => Ok("open"),
        Some("invite") => Ok("invite"),
        Some(other) => Err(invalid(format!(
            "joinPolicy is open, approval or invite, not {other}"
        ))),
    }
}

/// A new group: its handle, who starts it and runs it beside them, its
/// name and words, how people join, and whether anyone reads its meta
/// space.
#[derive(Debug, Clone, Default)]
pub struct NewGroup {
    pub handle: String,
    pub founder: String,
    pub stewards: Vec<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub join_policy: Option<String>,
    /// Whether anyone may read the meta space, beside its members: yes,
    /// unless it says.
    pub meta_public_read: Option<bool>,
}

/// A group made: its DID and its two spaces.
#[derive(Debug, Clone, PartialEq)]
pub struct Made {
    pub did: String,
    pub meta: String,
    pub members: String,
}

fn put(collection: &str, rkey: &str, value: Value) -> Write {
    Write::Create {
        collection: collection.into(),
        rkey: rkey.into(),
        value,
    }
}

/// A new group's policy for its meta space, readable by anyone if
/// `public`: its members read it. Its admins write its records by their
/// role's permissions across the group.
fn meta_policy(public: bool) -> Value {
    let mut policy = json!({"$type": POLICY, "grants": [
        {"role": "member", "read": true},
        {"role": "admin", "read": true},
    ]});
    if public {
        policy["publicRead"] = json!(true);
    }
    policy
}

/// A new group's policy for its members space: its members read it, and
/// write there, so that their confirmations sync.
fn members_policy() -> Value {
    json!({"$type": POLICY, "grants": [{"role": "member", "read": true, "write": true}]})
}

/// How a new group lets people in, whether anyone reads its meta, and who
/// runs it.
struct Shape {
    join: &'static str,
    public: bool,
    stewards: Vec<String>,
}

/// What `new` asks of a group's first day, checked.
fn shape(new: &NewGroup) -> Result<Shape, XrpcError> {
    let join = join_policy(new.join_policy.as_deref())?;
    let mut stewards = vec![new.founder.clone()];
    for s in &new.stewards {
        if !stewards.contains(s) {
            stewards.push(s.clone());
        }
    }
    Ok(Shape {
        join,
        public: new.meta_public_read != Some(false),
        stewards,
    })
}

impl Host {
    /// Whether this host starts groups for `did`: anyone, unless
    /// `GROUP_FOUNDERS` names who.
    pub fn may_found(&self, did: &str) -> bool {
        let founders = &self.config.group_founders;
        founders.is_empty() || founders.iter().any(|f| f == did)
    }

    /// Refuses `did` a group here, made, moved or brought, unless the host
    /// starts groups for them.
    pub(crate) fn founding(&self, did: &str) -> Result<(), XrpcError> {
        if self.may_found(did) {
            return Ok(());
        }
        Err(XrpcError::new(
            StatusCode::FORBIDDEN,
            "NotAllowed",
            "this host is not starting groups for you",
        ))
    }

    /// Makes a group: its account, its two spaces and what they hold from
    /// its first day, and its declaration if anyone may read its meta.
    pub async fn create_group(&self, new: &NewGroup) -> Result<Made, XrpcError> {
        self.founding(&new.founder)?;
        let shape = shape(new)?;
        let did = self.create_account(&new.handle).await.map_err(|e| {
            if e.error == "HandleNotAvailable" {
                XrpcError::new(e.status, "HandleTaken", e.message)
            } else {
                e
            }
        })?;
        // A group half made is no group: its account goes, so its handle
        // is free again. Should that fail, what was asked fails as it did,
        // and the host says why the handle stays taken.
        let made = self.set_up(&did, new, &shape).await;
        if made.is_err()
            && let Err(e) = self.delete_account(&did).await
        {
            tracing::warn!(did = %did, error = %format!("{e:#}"), "could not undo a group half made");
        }
        made
    }

    /// Makes `did`, an account here already and switched on, the group
    /// `new` asks for: its two spaces and what they hold from its first
    /// day, and its declaration if anyone may read its meta.
    pub async fn make_group(&self, did: &str, new: &NewGroup) -> Result<Made, XrpcError> {
        let shape = shape(new)?;
        self.set_up(did, new, &shape).await
    }

    async fn set_up(&self, did: &str, new: &NewGroup, shape: &Shape) -> Result<Made, XrpcError> {
        let Shape {
            join,
            public,
            stewards,
        } = shape;
        let did = did.to_string();
        let (meta_policy, members_policy) = (meta_policy(*public), members_policy());
        let meta = self.create_space(&did, &format!("{NS}.meta"), "self", &meta_policy)?;
        let members = self.create_space(&did, &format!("{NS}.members"), "self", &members_policy)?;
        let at = iso(now_millis());
        let c = |name: &str| format!("{NS}.{name}");
        let mut about = json!({"$type": c("about"), "displayName": new.display_name});
        if let Some(d) = &new.description {
            about["description"] = json!(d);
        }
        let config = json!({"$type": c("config"), "joinPolicy": join});
        let meta_writes = [
            put(&c("about"), "self", about),
            put(&c("config"), "self", config),
        ];
        self.space_write(&did, &meta, &meta_writes).await?;
        // Its founders run it: administering it, labeling, making,
        // changing and ending spaces, writing any record as it, in its
        // repo and in its spaces (so that they may give any permission a
        // space's policy may hold), and giving any role; everyone else is
        // a member.
        let administers = format!("include:{NS}.authManageAdmin");
        let permissions = [
            administers.as_str(),
            "space:*?collection=com.atproto.label.record",
            "space:*?manage=update",
            "space:*?manage=create&manage=delete",
            "space:*?collection=*",
            "repo:*",
        ];
        let mut members_writes = vec![
            put(
                &c("role"),
                "admin",
                json!({"$type": c("role"), "displayName": "Admin", "description": "Runs the group.", "permissions": permissions, "canAssign": [EVERY_ROLE]}),
            ),
            put(
                &c("role"),
                "member",
                json!({"$type": c("role"), "displayName": "Member", "default": true}),
            ),
            put(&c("labeler"), "self", labeler()),
        ];
        for steward in stewards {
            members_writes.push(put(
                &c("membership"),
                &did_rkey(steward),
                json!({"$type": c("membership"), "roles": ["admin", "member"], "createdAt": at}),
            ));
        }
        self.space_write(&did, &members, &members_writes).await?;
        // Each policy published; neither space is in the group's index.
        self.publish_policy(&did, &meta, &meta_policy).await?;
        self.publish_policy(&did, &members, &members_policy).await?;
        // Listed only if there is a face to show: a meta anyone may read.
        if *public {
            self.declare(&did, true).await?;
        }
        Ok(Made { did, meta, members })
    }

    /// Lists the group, by its declaration in its public repo, or not.
    pub async fn declare(&self, group: &str, listed: bool) -> Result<(), XrpcError> {
        let collection = format!("{NS}.declaration");
        let declared = self.store.record(group, &collection, "self");
        let declared = declared.map_err(XrpcError::internal)?.is_some();
        let write = match (listed, declared) {
            (true, false) => put(&collection, "self", json!({ "$type": collection })),
            (false, true) => Write::Delete {
                collection,
                rkey: "self".into(),
            },
            _ => return Ok(()),
        };
        self.write(group, &[write])?;
        Ok(())
    }

    /// The group's own record in `space`, as JSON.
    pub fn group_record(
        &self,
        group: &str,
        space: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<Option<Value>> {
        let found = self.store.space_record(group, space, collection, rkey)?;
        Ok(found.map(|r| json_of(&r)))
    }

    /// Every record of `collection` in the group's own repo in `space`,
    /// with its key, in key order.
    pub fn group_records(
        &self,
        group: &str,
        space: &str,
        collection: &str,
    ) -> Result<Vec<(String, Value)>> {
        let records =
            self.store
                .space_list(group, space, Some(collection), None, u32::MAX, true)?;
        Ok(records
            .iter()
            .map(|r| (r.rkey.clone(), json_of(r)))
            .collect())
    }

    /// The roles `did` holds in `group`: none for one not a member.
    pub fn roles(&self, group: &str, did: &str) -> Result<Vec<String>> {
        let (members, membership) = (members_space(group), format!("{NS}.membership"));
        let membership = self.group_record(group, &members, &membership, &did_rkey(did))?;
        Ok(membership.map(|m| roles_of(&m)).unwrap_or_default())
    }

    /// The group-wide grant of each of `group`'s roles, by its record.
    pub fn role_grants(&self, group: &str) -> Result<Vec<Grant>> {
        let roles = self.group_records(group, &members_space(group), &format!("{NS}.role"))?;
        Ok(grants(&roles))
    }
}

fn json_of(record: &Record) -> Value {
    drisl::decode(&record.value)
        .expect("a record the host wrote")
        .to_json()
}
