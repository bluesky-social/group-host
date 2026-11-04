//! Moderation, a group's own: reports in, labels out, a queue between.
//!
//! - **Labels** are records the group writes into the space its subject
//!   lives in, so whoever reads the content reads its labels. An account
//!   has no content to sit beside: its labels go in the members space. A
//!   label is taken back by a second record that negates it, not by
//!   deleting the first: that it was applied is part of the record.
//! - **The queue** (subjects, their status, what happened to each) is the
//!   host's to keep, not the group's to publish.
//! - **Reports** come from anyone, members or not
//!   (`com.atproto.moderation.createReport`), on a record in one of a
//!   group's spaces here.
//!
//! As the prototype's `methods/moderation.ts`, less the host's own queue,
//! which moderated what its sites module shared.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::group::members_space;
use crate::group_api::{group_of, signed_in};
use crate::rbac::NS;
use crate::repo::{Write, record_cid};
use crate::store::Store;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::syntax;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use rusqlite::{OptionalExtension, Row, params};
use serde_json::{Value, json};
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// The values that remove content, for a role holding `takedown`.
const TAKEDOWN_VALS: [&str; 2] = ["!hide", "!takedown"];

/// A subject in a group's queue.
#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub subject: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub reports: i64,
    pub last_reported_at: Option<i64>,
}

/// Something that happened to a subject.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Event {
    pub kind: String,
    pub actor: Option<String>,
    pub reason_type: Option<String>,
    pub reason: Option<String>,
    pub val: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
}

const SUBJECT_COLUMNS: &str = "s.subject, s.status, s.created_at, s.updated_at, (SELECT count(*) FROM mod_event e WHERE e.grp = s.grp AND e.subject = s.subject AND e.kind = 'report'), (SELECT max(created_at) FROM mod_event e WHERE e.grp = s.grp AND e.subject = s.subject AND e.kind = 'report')";

fn subject_row(row: &Row) -> rusqlite::Result<Subject> {
    Ok(Subject {
        subject: row.get(0)?,
        status: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        reports: row.get(4)?,
        last_reported_at: row.get(5)?,
    })
}

fn event_row(row: &Row) -> rusqlite::Result<Event> {
    Ok(Event {
        kind: row.get(0)?,
        actor: row.get(1)?,
        reason_type: row.get(2)?,
        reason: row.get(3)?,
        val: row.get(4)?,
        note: row.get(5)?,
        created_at: row.get(6)?,
    })
}

impl Store {
    /// Keeps `event` on `subject` in `group`'s queue, opening the subject
    /// if it is new; a report reopens one resolved, as someone asks again.
    pub fn moderate(&self, group: &str, subject: &str, event: &Event) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let at = event.created_at;
        let reopen = event.kind == "report";
        tx.execute(
            "INSERT INTO mod_subject (grp, subject, status, created_at, updated_at) VALUES (?1, ?2, 'open', ?3, ?3)
             ON CONFLICT (grp, subject) DO UPDATE SET updated_at = ?3,
             status = CASE WHEN ?4 AND status = 'resolved' THEN 'open' ELSE status END",
            params![group, subject, at, reopen],
        )?;
        let e = event;
        tx.execute(
            "INSERT INTO mod_event (grp, subject, kind, actor, reason_type, reason, val, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![group, subject, e.kind, e.actor, e.reason_type, e.reason, e.val, e.note, at],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Sets `subject`'s status in `group`'s queue: `false` if it has none.
    pub fn set_status(&self, group: &str, subject: &str, status: &str, at: i64) -> Result<bool> {
        let conn = self.conn();
        let sql =
            "UPDATE mod_subject SET status = ?3, updated_at = ?4 WHERE grp = ?1 AND subject = ?2";
        Ok(conn.execute(sql, params![group, subject, status, at])? > 0)
    }

    /// `subject` in `group`'s queue.
    pub fn subject(&self, group: &str, subject: &str) -> Result<Option<Subject>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {SUBJECT_COLUMNS} FROM mod_subject s WHERE s.grp = ?1 AND s.subject = ?2"
        );
        Ok(conn
            .query_row(&sql, params![group, subject], subject_row)
            .optional()?)
    }

    /// `group`'s queue, of `status` if named, the latest changed first,
    /// after `before` (an `updated_at` and subject), at most `limit`.
    pub fn subjects(
        &self,
        group: &str,
        status: Option<&str>,
        before: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<Vec<Subject>> {
        let conn = self.conn();
        let (at, after) = before.unwrap_or((i64::MAX, ""));
        let sql = format!(
            "SELECT {SUBJECT_COLUMNS} FROM mod_subject s WHERE s.grp = ?1 AND (?2 IS NULL OR s.status = ?2)
             AND (s.updated_at < ?3 OR (s.updated_at = ?3 AND s.subject > ?4))
             ORDER BY s.updated_at DESC, s.subject LIMIT ?5"
        );
        let mut q = conn.prepare(&sql)?;
        let rows = q.query_map(params![group, status, at, after, limit], subject_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What happened to `subject` in `group`'s queue, in order.
    pub fn events(&self, group: &str, subject: &str) -> Result<Vec<Event>> {
        let conn = self.conn();
        let sql = "SELECT kind, actor, reason_type, reason, val, note, created_at FROM mod_event WHERE grp = ?1 AND subject = ?2 ORDER BY seq";
        let mut q = conn.prepare(sql)?;
        let rows = q.query_map(params![group, subject], event_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

fn not_found() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "SubjectNotFound",
        "no such subject in the queue",
    )
}

/// The space a record's URI is in, `at://<authority>/space/<type>/<skey>`,
/// and its authority, for a URI of a record inside one, each of its parts
/// what it says: so a URI anyone sends is bounded, as its parts are.
pub fn space_of(uri: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = uri.strip_prefix("at://")?.split('/').collect();
    let [authority, "space", kind, skey, repo, collection, rkey] = parts[..] else {
        return None;
    };
    let dids = syntax::is_did(authority) && syntax::is_did(repo);
    let names = syntax::is_nsid(kind) && syntax::is_nsid(collection);
    let keys = syntax::is_record_key(skey) && syntax::is_record_key(rkey);
    (dids && names && keys).then(|| {
        let space = format!("at://{authority}/space/{kind}/{skey}");
        (space, authority.to_string())
    })
}

/// The longest reason a report gives, as `createReport`'s lexicon has it.
const REASON_MAX: usize = 20_000;
/// The longest reason type a report gives: one of the known ones, or
/// another as long.
const REASON_TYPE_MAX: usize = 256;

/// A subject as a label holds it, and as the queue keys it: an account's
/// DID, or a record's URI.
fn subject_of(asked: &Value) -> Result<(Value, String), XrpcError> {
    if let Some(did) = asked["did"].as_str() {
        if !syntax::is_did(did) {
            return Err(invalid("subject.did is not a DID"));
        }
        return Ok((
            json!({"$type": "com.atproto.admin.defs#repoRef", "did": did}),
            did.to_string(),
        ));
    }
    let (Some(uri), Some(cid)) = (asked["uri"].as_str(), asked["cid"].as_str()) else {
        return Err(invalid(
            "a subject is an account's did, or a record's uri and cid",
        ));
    };
    Ok((
        json!({"$type": "com.atproto.repo.strongRef", "uri": uri, "cid": cid}),
        uri.to_string(),
    ))
}

impl Host {
    /// The group's queue, for `me` holding `action` in it.
    async fn moderator(
        &self,
        headers: &HeaderMap,
        lxm: &str,
        group: &str,
        action: &str,
    ) -> Result<String, XrpcError> {
        let me = signed_in(self, headers, &format!("{NS}.{lxm}")).await?;
        group_of(self, group)?;
        if !self.may_do(group, &me, action)? {
            return Err(forbidden(format!("missing {action}")));
        }
        Ok(me)
    }

    /// Writes a label (or its negation) on `subject` as `group`, in the
    /// space it lives in, and notes it in the queue.
    async fn label(&self, headers: &HeaderMap, lxm: &str, body: &Value, neg: bool) -> Answer {
        let group = body["group"]
            .as_str()
            .ok_or_else(|| invalid("group is required"))?;
        let val = body["val"].as_str().filter(|v| !v.is_empty());
        let val = val.ok_or_else(|| invalid("val is required"))?;
        let action = if TAKEDOWN_VALS.contains(&val) {
            "takedown"
        } else {
            "label"
        };
        let me = self.moderator(headers, lxm, group, action).await?;
        let (subject, key) = subject_of(&body["subject"])?;
        let space = match subject["uri"].as_str() {
            None => members_space(group),
            Some(uri) => {
                let space = space_of(uri);
                let space = space.ok_or_else(|| {
                    invalid("a record subject is one in a space, for its label to sit beside it")
                })?;
                // Another group's content is that group's to moderate.
                if space.1 != group {
                    return Err(forbidden("subject is not in this group's spaces"));
                }
                space.0
            }
        };
        let collection = format!("{NS}.label");
        let mut value = json!({"$type": collection, "subject": subject, "val": val, "createdAt": iso(now_millis())});
        if neg {
            value["neg"] = json!(true);
        }
        if let Some(rule) = body["rule"].as_str() {
            value["rule"] = json!(rule);
        }
        let rkey = self.clock.next();
        let cid = record_cid(&collection, &value)?;
        let write = Write::Create {
            collection: collection.clone(),
            rkey: rkey.clone(),
            value,
        };
        self.space_write(group, &space, &[write]).await?;
        let event = Event {
            kind: if neg { "negate" } else { "label" }.into(),
            actor: Some(me),
            val: Some(val.into()),
            created_at: now_millis(),
            ..Event::default()
        };
        self.store
            .moderate(group, &key, &event)
            .map_err(XrpcError::internal)?;
        let uri = format!("{space}/{group}/{collection}/{rkey}");
        Ok(Json(json!({"uri": uri, "cid": cid.to_string()})))
    }
}

/// Labels a subject in one of the group's spaces, or an account.
pub async fn apply_label(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    host.label(&headers, "applyLabel", &body, false).await
}

/// Negates a label, with a second record.
pub async fn negate_label(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    // Its record is not the answer: a negation answers nothing.
    let _negation = host.label(&headers, "negateLabel", &body, true).await?;
    Ok(Json(json!({})))
}

fn subject_view(s: &Subject) -> Value {
    let mut view = json!({
        "subject": s.subject,
        "status": s.status,
        "reportCount": s.reports,
        "createdAt": iso(s.created_at),
        "updatedAt": iso(s.updated_at),
    });
    if let Some(at) = s.last_reported_at {
        view["lastReportedAt"] = json!(iso(at));
    }
    view
}

/// The group's queue, of a status if named, the latest changed first.
pub async fn list_subjects(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    host.moderator(&headers, "listSubjects", group, "mod.read")
        .await?;
    let limit = page(&p, 50, 100)?;
    let cursor = p
        .get("cursor")
        .map(|c| c.split_once(':').unwrap_or((c, "")));
    let before = match cursor {
        None => None,
        Some((at, subject)) => Some((
            at.parse::<i64>()
                .map_err(|_| invalid("a cursor this host gave"))?,
            subject,
        )),
    };
    let status = p.get("status").map(String::as_str);
    let subjects = host
        .store
        .subjects(group, status, before, limit)
        .map_err(XrpcError::internal)?;
    let mut out = json!({"subjects": subjects.iter().map(subject_view).collect::<Vec<_>>()});
    if subjects.len() == limit as usize
        && let Some(last) = subjects.last()
    {
        out["cursor"] = json!(format!("{}:{}", last.updated_at, last.subject));
    }
    Ok(Json(out))
}

/// A subject in the group's queue, and what happened to it.
pub async fn get_subject_history(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    host.moderator(&headers, "getSubjectHistory", group, "mod.read")
        .await?;
    let subject = param(&p, "subject")?;
    let found = host
        .store
        .subject(group, subject)
        .map_err(XrpcError::internal)?;
    let found = found.ok_or_else(not_found)?;
    let events = host
        .store
        .events(group, subject)
        .map_err(XrpcError::internal)?;
    let events: Vec<Value> = events
        .iter()
        .map(|e| {
            let mut view = json!({"kind": e.kind, "createdAt": iso(e.created_at)});
            for (k, v) in [
                ("actor", &e.actor),
                ("reasonType", &e.reason_type),
                ("reason", &e.reason),
                ("val", &e.val),
                ("note", &e.note),
            ] {
                if let Some(v) = v {
                    view[k] = json!(v);
                }
            }
            view
        })
        .collect();
    Ok(Json(
        json!({"subject": subject_view(&found), "events": events}),
    ))
}

/// Resolves a subject, escalates it, or notes something on it.
pub async fn resolve_subject(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"]
        .as_str()
        .ok_or_else(|| invalid("group is required"))?;
    let me = host
        .moderator(&headers, "resolveSubject", group, "mod.resolve")
        .await?;
    let subject = body["subject"]
        .as_str()
        .ok_or_else(|| invalid("subject is required"))?;
    let action = body["action"].as_str().unwrap_or_default();
    let status = match action {
        "resolve" => Some("resolved"),
        "escalate" => Some("escalated"),
        "note" => None,
        other => {
            return Err(invalid(format!(
                "action is resolve, escalate or note, not {other}"
            )));
        }
    };
    let now = now_millis();
    if host
        .store
        .subject(group, subject)
        .map_err(XrpcError::internal)?
        .is_none()
    {
        return Err(not_found());
    }
    // A note is said of the subject; it changes nothing.
    if let Some(status) = status {
        host.store
            .set_status(group, subject, status, now)
            .map_err(XrpcError::internal)?;
    }
    let event = Event {
        kind: action.into(),
        actor: Some(me),
        note: body["note"].as_str().map(String::from),
        created_at: now,
        ..Event::default()
    };
    host.store
        .moderate(group, subject, &event)
        .map_err(XrpcError::internal)?;
    Ok(Json(json!({})))
}

/// A report, from anyone: of a record in one of a group's spaces here,
/// into that group's queue.
pub async fn create_report(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, "com.atproto.moderation.createReport").await?;
    let reason_type = body["reasonType"]
        .as_str()
        .ok_or_else(|| invalid("reasonType is required"))?;
    if reason_type.len() > REASON_TYPE_MAX {
        return Err(invalid(format!(
            "reasonType is {REASON_TYPE_MAX} bytes at most"
        )));
    }
    if body["reason"]
        .as_str()
        .is_some_and(|r| r.len() > REASON_MAX)
    {
        return Err(invalid(format!("reason is {REASON_MAX} bytes at most")));
    }
    let uri = body["subject"]["uri"].as_str();
    let uri = uri.ok_or_else(|| invalid("a report here is of a record, by its uri"))?;
    let space =
        space_of(uri).ok_or_else(|| invalid("the subject is not a record in a group's space"))?;
    let group = space.1;
    let hosted = host.store.space(&space.0).map_err(XrpcError::internal)?;
    if hosted.is_none_or(|s| s.authority != group) {
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "NotHosted",
            "that space is not one of a group's here",
        ));
    }
    let now = now_millis();
    let event = Event {
        kind: "report".into(),
        actor: Some(me.clone()),
        reason_type: Some(reason_type.into()),
        reason: body["reason"].as_str().map(String::from),
        created_at: now,
        ..Event::default()
    };
    host.store
        .moderate(&group, uri, &event)
        .map_err(XrpcError::internal)?;
    let mut out = json!({
        "id": now,
        "reasonType": reason_type,
        "subject": body["subject"],
        "reportedBy": me,
        "createdAt": iso(now),
    });
    if let Some(reason) = body["reason"].as_str() {
        out["reason"] = json!(reason);
    }
    Ok(Json(out))
}
