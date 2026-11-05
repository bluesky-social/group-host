//! A group's members space, written as the group: an app a member signed
//! in, writing roles, the permissions record and memberships there.

#![allow(dead_code)]

use super::Running;
use super::groups::NS;
use super::oauth::{App, Said};
use super::spaces::Authority;
use group_host::group::members_space;
use group_host::repo::Write;
use serde_json::{Value as Json, json};

/// What an app asks for to run a group's members: its roles and what
/// each may do, and who holds them.
pub const RUNS: &str =
    "atproto include:fyi.opensocial.configure include:fyi.opensocial.manageMembers";

/// The collection `name` of ours.
pub fn c(name: &str) -> String {
    format!("{NS}.{name}")
}

/// A role record, named `name`.
pub fn role(name: &str) -> Json {
    json!({"$type": c("role"), "displayName": name, "createdAt": "2026-11-03T00:00:00Z"})
}

/// A membership record of `did`, holding `roles`.
pub fn membership(did: &str, roles: &[&str]) -> Json {
    json!({"$type": c("membership"), "member": did, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"})
}

/// A record of `group`'s members space, as kept.
pub fn record(h: &Running, group: &str, name: &str, rkey: &str) -> Option<Json> {
    let space = members_space(group);
    h.host.group_record(group, &space, &c(name), rkey).unwrap()
}

/// `group`'s permissions record with `role`'s grant as given, in place of
/// any it had.
pub fn granting(
    h: &Running,
    group: &str,
    role: &str,
    scopes: &[&str],
    assignable: &[&str],
) -> Json {
    let mut p = record(h, group, "permissions", "self").unwrap();
    let grants = p["grants"].as_array_mut().unwrap();
    grants.retain(|g| g["role"] != role);
    grants.push(json!({"role": role, "scopes": scopes, "assignable": assignable}));
    p
}

/// Lets holders of `role`, in the permissions record `p`, give `another`.
pub fn gives(p: &mut Json, role: &str, another: &str) {
    for g in p["grants"].as_array_mut().unwrap() {
        if g["role"] == role {
            g["assignable"].as_array_mut().unwrap().push(json!(another));
        }
    }
}

/// Writes `record` at `rkey` in `group`'s members space, as the host
/// would itself, asking nothing.
pub async fn keep(h: &Running, group: &str, name: &str, rkey: &str, record: Json) {
    let space = members_space(group);
    let there = h.host.group_record(group, &space, &c(name), rkey).unwrap();
    let (collection, rkey) = (c(name), rkey.to_string());
    let write = match there {
        Some(_) => Write::Update {
            collection,
            rkey,
            value: record,
        },
        None => Write::Create {
            collection,
            rkey,
            value: record,
        },
    };
    h.host.space_write(group, &space, &[write]).await.unwrap();
}

/// An app signed in as a group by one of its members, asking `scope`.
pub struct Session {
    pub app: App,
    pub token: String,
    pub group: String,
}

impl Session {
    /// `who` signs an app asking `scope` in as `group`.
    pub async fn of(h: &Running, who: &Authority, group: &str, scope: &str) -> Session {
        let app = App::loopback(scope);
        let tokens = app.sign_in(h, who, group).await;
        let token = tokens["access_token"].as_str().unwrap().to_string();
        Session {
            app,
            token,
            group: group.into(),
        }
    }

    /// `who` signs in an app that runs `group`'s members.
    pub async fn runs(h: &Running, who: &Authority, group: &str) -> Session {
        Session::of(h, who, group, RUNS).await
    }

    async fn call(&self, h: &Running, method: &str, mut body: Json) -> Said {
        body["repo"] = json!(self.group);
        body["space"] = json!(members_space(&self.group));
        let nsid = format!("com.atproto.space.{method}");
        self.app.call(h, &nsid, &self.token, Some(&body)).await
    }

    /// Puts `record` at `rkey`, of our collection `name`.
    pub async fn put(&self, h: &Running, name: &str, rkey: &str, record: Json) -> Said {
        let body = json!({"collection": c(name), "rkey": rkey, "record": record});
        self.call(h, "putRecord", body).await
    }

    /// Creates `record` at `rkey`, of our collection `name`.
    pub async fn create(&self, h: &Running, name: &str, rkey: &str, record: Json) -> Said {
        let body = json!({"collection": c(name), "rkey": rkey, "record": record});
        self.call(h, "createRecord", body).await
    }

    /// Deletes the record at `rkey`, of our collection `name`.
    pub async fn delete(&self, h: &Running, name: &str, rkey: &str) -> Said {
        let body = json!({"collection": c(name), "rkey": rkey});
        self.call(h, "deleteRecord", body).await
    }

    /// Makes `writes` at once: each `(action, name, rkey, record)`.
    pub async fn apply(&self, h: &Running, writes: &[(&str, &str, &str, Json)]) -> Said {
        let writes: Vec<Json> = writes
            .iter()
            .map(|(action, name, rkey, value)| {
                let mut w = json!({
                    "$type": format!("com.atproto.space.applyWrites#{action}"),
                    "collection": c(name),
                    "rkey": rkey,
                });
                if *action != "delete" {
                    w["value"] = value.clone();
                }
                w
            })
            .collect();
        self.call(h, "applyWrites", json!({ "writes": writes }))
            .await
    }
}
