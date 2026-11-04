//! Groups: an account here with two spaces of its own, `meta` (its
//! profile, rules and labeler, for whoever it lets read them) and
//! `members` (its roles, what each may do, its memberships and the index
//! of its spaces, for its members). Everything a group is lives in those
//! records, written by the group; who may read or write each space is its
//! access record, against the reader's roles.

use crate::Host;
use crate::api::at_most;
use crate::rbac::{ACTIONS, Grant, NS, listed};
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

/// What the labeler every group starts with says: the labels the host
/// acts on, and reports about records.
fn labeler(at: &str) -> Value {
    let reasons = ["Spam", "Violation", "Misleading", "Sexual", "Rude", "Other"]
        .map(|r| format!("com.atproto.moderation.defs#reason{r}"));
    json!({
        "$type": format!("{NS}.labeler"),
        "policies": {"labelValues": ["!hide", "!takedown"]},
        "reasonTypes": reasons,
        "subjectTypes": ["record"],
        "createdAt": at,
    })
}

/// How a new group lets people in, as its profile says it: `open`,
/// `approval` or `invite`, from either the profile's words or
/// `createGroup`'s (`fyi.opensocial.profile#request`, …); `approval`
/// unless it says.
pub fn join_policy(asked: Option<&str>) -> Result<&'static str, XrpcError> {
    let word = asked.map(|a| a.rsplit('#').next().unwrap_or(a));
    match word {
        None | Some("approval") | Some("request") => Ok("approval"),
        Some("open") => Ok("open"),
        Some("invite") => Ok("invite"),
        Some(other) => Err(invalid(format!(
            "joinPolicy is open, approval or invite, not {other}"
        ))),
    }
}

/// A new group: its handle, who starts it and runs it beside them, its
/// name and words, how people join, and who reads its meta space.
#[derive(Debug, Clone, Default)]
pub struct NewGroup {
    pub handle: String,
    pub founder: String,
    pub stewards: Vec<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub join_policy: Option<String>,
    /// The roles that read the meta space, `public` for anyone signed in;
    /// `public` unless it says.
    pub meta_readable_by: Option<Vec<String>>,
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

fn audience(roles: &[String]) -> Value {
    roles.iter().map(|r| json!({ "role": r })).collect()
}

/// How a new group lets people in, who reads its meta, and who runs it.
struct Shape {
    join: &'static str,
    readers: Vec<String>,
    stewards: Vec<String>,
}

/// What `new` asks of a group's first day, checked.
fn shape(new: &NewGroup) -> Result<Shape, XrpcError> {
    let join = join_policy(new.join_policy.as_deref())?;
    at_most("displayName", &new.display_name, 640, 64)?;
    if let Some(d) = &new.description {
        at_most("description", d, 3000, 300)?;
    }
    let readers = new
        .meta_readable_by
        .clone()
        .unwrap_or_else(|| vec!["public".into()]);
    let mut stewards = vec![new.founder.clone()];
    for s in &new.stewards {
        if !atproto::syntax::is_did(s) {
            return Err(invalid(format!("steward {s} is no DID")));
        }
        if !stewards.contains(s) {
            stewards.push(s.clone());
        }
    }
    Ok(Shape {
        join,
        readers,
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
            readers,
            stewards,
        } = shape;
        let did = did.to_string();
        let meta = self.create_space(&did, &format!("{NS}.meta"), "self", false)?;
        let members = self.create_space(&did, &format!("{NS}.members"), "self", false)?;
        let at = iso(now_millis());
        let c = |name: &str| format!("{NS}.{name}");
        let mut profile = json!({
            "$type": c("profile"),
            "displayName": new.display_name,
            "joinPolicy": join,
            "createdAt": at,
        });
        if let Some(d) = &new.description {
            profile["description"] = json!(d);
        }
        let meta_writes = [
            put(&c("profile"), "self", profile),
            put(&c("labeler"), "self", labeler(&at)),
            // Admins write the profile and rules as the group.
            put(
                &c("access"),
                "self",
                json!({
                    "$type": c("access"),
                    "readableBy": audience(readers),
                    "credentialScopes": [{"role": "admin", "scopes": [{"collection": c("profile")}, {"collection": c("rule")}]}],
                    "createdAt": at,
                }),
            ),
        ];
        self.space_write(&did, &meta, &meta_writes).await?;
        let admin = Grant::with("admin", &ACTIONS, &["member"], &["*"]);
        let member = Grant::with("member", &[], &[], &[]);
        let mut members_writes = vec![
            put(
                &c("access"),
                "self",
                json!({"$type": c("access"), "readableBy": [{"role": "member"}], "createdAt": at}),
            ),
            put(
                &c("role"),
                "admin",
                json!({"$type": c("role"), "displayName": "Admin", "description": "Runs the group.", "createdAt": at}),
            ),
            put(
                &c("role"),
                "member",
                json!({"$type": c("role"), "displayName": "Member", "default": true, "createdAt": at}),
            ),
            put(
                &c("permissions"),
                "self",
                json!({"$type": c("permissions"), "grants": [admin.to_json(), member.to_json()], "createdAt": at}),
            ),
        ];
        // The index of the group's spaces, which apps read to find them.
        for space in [&meta, &members] {
            members_writes.push(put(
                &c("space"),
                &self.clock.next(),
                json!({"$type": c("space"), "space": space, "createdAt": at}),
            ));
        }
        for steward in stewards {
            members_writes.push(put(
                &c("membership"),
                &did_rkey(steward),
                json!({"$type": c("membership"), "member": steward, "roles": [{"role": "admin"}, {"role": "member"}], "createdAt": at}),
            ));
        }
        self.space_write(&did, &members, &members_writes).await?;
        // Listed only if there is a face to show: a meta anyone may read.
        if readers.iter().any(|r| r == "public") {
            let declaration = json!({"$type": c("declaration"), "meta": meta, "createdAt": at});
            self.write(&did, &[put(&c("declaration"), "self", declaration)])?;
        }
        Ok(Made { did, meta, members })
    }

    /// Lists the group, by a declaration in its public repo pointing at
    /// its meta space, or not.
    pub async fn declare(&self, group: &str, listed: bool) -> Result<(), XrpcError> {
        let collection = format!("{NS}.declaration");
        let declared = self.store.record(group, &collection, "self");
        let declared = declared.map_err(XrpcError::internal)?.is_some();
        let write = match (listed, declared) {
            (true, false) => put(
                &collection,
                "self",
                json!({"$type": collection, "meta": meta_space(group), "createdAt": iso(now_millis())}),
            ),
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
        Ok(membership
            .map(|m| listed(&m, "roles", "role"))
            .unwrap_or_default())
    }
}

fn json_of(record: &Record) -> Value {
    drisl::decode(&record.value)
        .expect("a record the host wrote")
        .to_json()
}
