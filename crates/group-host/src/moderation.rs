//! Moderation, a group's own: reports in, labels out, a queue between.
//!
//! - **Labels** are `com.atproto.label.record` records the group writes,
//!   as any record of its own, beside their subject: a record's in the
//!   space it is in, an account's, or a member's public record's, in the
//!   members space by custom. Whoever reads the content reads its labels.
//!   Deleting a label takes it back; the queue keeps that it was written,
//!   and by whom.
//! - **The queue** (subjects, their status and tags, what happened to
//!   each) is the host's to keep, not the group's to publish.
//! - **Reports** come through `org.atgrp.mod.submitReport`, from whoever
//!   can read what they report: an account in the group, a record in one
//!   of its spaces here, or a member's record in their public repo.
//! - **Appeals** come through `org.atgrp.mod.submitAppeal`, from the
//!   account a label or an ejection is on, or the author of the record
//!   labelled, though they have left; `org.atgrp.mod.querySelfHistory`
//!   shows them what the group did, and nothing of who did it.

use crate::Host;
use crate::api::{Params, invalid, invalid_cursor, page, param};
use crate::authority::Access;
use crate::blobs::blob_refs;
use crate::group::members_space;
use crate::group_api::{doing, group_of, signed_in};
use crate::index_api::served;
use crate::oauth_resource::Refusal;
use crate::rbac::{INTERNAL, LABEL, NS};
use crate::repo::Write;
use crate::store::Store;
use crate::takedown::Reviewed;
use crate::time::{iso, now_millis};
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::space::{SPACE_URI_PREFIX, SpaceRecordUri};
use atproto::{Cid, drisl, syntax};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use rusqlite::{OptionalExtension, Row, params};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// The `$type` of the moderation views and events, `org.atgrp.mod.defs`.
const DEFS: &str = "org.atgrp.mod.defs";

/// A subject in a group's queue: an account's DID, or a record's URI.
#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub subject: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub reports: i64,
    /// Its moderators' tags, in order.
    pub tags: Vec<String>,
}

/// Something that happened to a subject: a `report`, an `appeal`, a
/// `status` set, `tag`s added or taken away, a `label` written or taken
/// back (`negate`), or an `eject`ion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Event {
    /// Its place in the host's history, once kept.
    pub seq: i64,
    pub kind: String,
    pub actor: Option<String>,
    /// The version of a record subject it is about.
    pub cid: Option<String>,
    pub reason_type: Option<String>,
    pub reason: Option<String>,
    /// The rule a report cites.
    pub rule: Option<String>,
    /// The status a moderator set.
    pub status: Option<String>,
    pub val: Option<String>,
    /// The label record written.
    pub uri: Option<String>,
    pub note: Option<String>,
    /// The tags a moderator added, and those taken away.
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub created_at: i64,
}

const SUBJECT_COLUMNS: &str = "s.subject, s.status, s.created_at, s.updated_at,
    (SELECT count(*) FROM mod_event e WHERE e.grp = s.grp AND e.subject = s.subject AND e.kind = 'report'),
    (SELECT group_concat(t.tag, ',' ORDER BY t.tag) FROM mod_tag t WHERE t.grp = s.grp AND t.subject = s.subject)";

const EVENT_COLUMNS: &str = "seq, kind, actor, cid, reason_type, reason, rule, status, val, uri, note, created_at, added, removed, subject";

/// Tags as a column keeps them: joined by commas, which a tag never holds.
fn joined(tags: &[String]) -> Option<String> {
    (!tags.is_empty()).then(|| tags.join(","))
}

/// Tags as a column kept them.
fn split(tags: Option<String>) -> Vec<String> {
    tags.map(|t| t.split(',').map(String::from).collect())
        .unwrap_or_default()
}

fn subject_row(row: &Row) -> rusqlite::Result<Subject> {
    Ok(Subject {
        subject: row.get(0)?,
        status: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        reports: row.get(4)?,
        tags: split(row.get(5)?),
    })
}

/// An event, and the subject it happened to.
fn event_row(row: &Row) -> rusqlite::Result<(String, Event)> {
    let event = Event {
        seq: row.get(0)?,
        kind: row.get(1)?,
        actor: row.get(2)?,
        cid: row.get(3)?,
        reason_type: row.get(4)?,
        reason: row.get(5)?,
        rule: row.get(6)?,
        status: row.get(7)?,
        val: row.get(8)?,
        uri: row.get(9)?,
        note: row.get(10)?,
        created_at: row.get(11)?,
        add: split(row.get(12)?),
        remove: split(row.get(13)?),
    };
    Ok((row.get(14)?, event))
}

/// A history's order, the newest first or the oldest, as SQL compares
/// and sorts by `seq`; and where a page after the event `after` starts.
fn order(newest: bool, after: Option<i64>) -> (&'static str, &'static str, i64) {
    match newest {
        true => ("<", "DESC", after.unwrap_or(i64::MAX)),
        false => (">", "ASC", after.unwrap_or(0)),
    }
}

impl Store {
    /// Keeps `event` on `subject` in `group`'s queue, opening the subject
    /// if it is new. A status set is the subject's, and tags its to have
    /// or lose; a report or an appeal reopens one resolved, as someone
    /// asks again.
    pub fn moderate(&self, group: &str, subject: &str, event: &Event) -> Result<i64> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let e = event;
        let reopen = matches!(e.kind.as_str(), "report" | "appeal");
        tx.execute(
            "INSERT INTO mod_subject (grp, subject, status, created_at, updated_at) VALUES (?1, ?2, 'open', ?3, ?3)
             ON CONFLICT (grp, subject) DO UPDATE SET updated_at = ?3,
             status = coalesce(?5, CASE WHEN ?4 AND status = 'resolved' THEN 'open' ELSE status END)",
            params![group, subject, e.created_at, reopen, e.status],
        )?;
        for tag in &e.add {
            tx.execute(
                "INSERT INTO mod_tag (grp, subject, tag) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
                params![group, subject, tag],
            )?;
        }
        for tag in &e.remove {
            tx.execute(
                "DELETE FROM mod_tag WHERE grp = ?1 AND subject = ?2 AND tag = ?3",
                params![group, subject, tag],
            )?;
        }
        tx.execute(
            "INSERT INTO mod_event (grp, subject, kind, actor, cid, reason_type, reason, rule, status, val, uri, note, added, removed, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                group,
                subject,
                e.kind,
                e.actor,
                e.cid,
                e.reason_type,
                e.reason,
                e.rule,
                e.status,
                e.val,
                e.uri,
                e.note,
                joined(&e.add),
                joined(&e.remove),
                e.created_at
            ],
        )?;
        let seq = tx.last_insert_rowid();
        tx.commit()?;
        Ok(seq)
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

    /// `group`'s queue, of `status` if named, with any of `tags` if any
    /// are named, in the order `sort` names: `oldest` or `newest` made,
    /// or the latest `updated` first; after `after` (the time it sorts by,
    /// and a subject), at most `limit`.
    pub fn subjects(
        &self,
        group: &str,
        status: Option<&str>,
        tags: &[String],
        sort: &str,
        after: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<Vec<Subject>> {
        let (column, op, order, start) = match sort {
            "newest" => ("created_at", "<", "DESC", i64::MAX),
            "updated" => ("updated_at", "<", "DESC", i64::MAX),
            _ => ("created_at", ">", "ASC", i64::MIN),
        };
        let (at, past) = after.unwrap_or((start, ""));
        let tags = (!tags.is_empty()).then(|| json!(tags).to_string());
        let conn = self.conn();
        let sql = format!(
            "SELECT {SUBJECT_COLUMNS} FROM mod_subject s WHERE s.grp = ?1
             AND (?2 IS NULL OR s.status = ?2)
             AND (?3 IS NULL OR EXISTS (SELECT 1 FROM mod_tag t WHERE t.grp = s.grp
                 AND t.subject = s.subject AND t.tag IN (SELECT value FROM json_each(?3))))
             AND (s.{column} {op} ?4 OR (s.{column} = ?4 AND s.subject > ?5))
             ORDER BY s.{column} {order}, s.subject LIMIT ?6"
        );
        let mut q = conn.prepare(&sql)?;
        let rows = q.query_map(params![group, status, tags, at, past, limit], subject_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What happened to `subject` in `group`'s queue, the `newest` first
    /// or the oldest: of the version `cid` alone if named, after the
    /// event `after`, at most `limit`.
    pub fn events(
        &self,
        group: &str,
        subject: &str,
        cid: Option<&str>,
        newest: bool,
        after: Option<i64>,
        limit: u32,
    ) -> Result<Vec<Event>> {
        let (op, order, after) = order(newest, after);
        let conn = self.conn();
        let sql = format!(
            "SELECT {EVENT_COLUMNS} FROM mod_event WHERE grp = ?1 AND subject = ?2
             AND (?3 IS NULL OR cid = ?3) AND seq {op} ?4 ORDER BY seq {order} LIMIT ?5"
        );
        let mut q = conn.prepare(&sql)?;
        let rows = q.query_map(params![group, subject, cid, after, limit], event_row)?;
        let rows = rows.map(|r| r.map(|(_, e)| e));
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The subjects in `group`'s queue that are `account`, or a record of
    /// theirs: in a public repo, or in one of the group's spaces.
    pub fn subjects_of(&self, group: &str, account: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let public = format!("at://{account}/");
        let spaces = format!("{SPACE_URI_PREFIX}{group}/");
        let sql = "SELECT subject FROM mod_subject WHERE grp = ?1 AND (subject = ?2
             OR substr(subject, 1, length(?3)) = ?3 OR substr(subject, 1, length(?4)) = ?4)
             ORDER BY subject";
        let mut q = conn.prepare(sql)?;
        let rows = q.query_map(params![group, account, public, spaces], |r| r.get(0))?;
        let subjects: Vec<String> = rows.collect::<rusqlite::Result<_>>()?;
        // A record in a space is its author's by the repo its URI names.
        let theirs = subjects.into_iter().filter(|s| account_of(s) == account);
        Ok(theirs.collect())
    }

    /// What happened to `subjects` in `group`'s queue that its subject
    /// sees: neither reports nor tags. The `newest` first or the oldest,
    /// after the event `after`, at most `limit`; each with its subject.
    pub fn own_events(
        &self,
        group: &str,
        subjects: &[String],
        newest: bool,
        after: Option<i64>,
        limit: u32,
    ) -> Result<Vec<(String, Event)>> {
        let (op, order, after) = order(newest, after);
        let conn = self.conn();
        let sql = format!(
            "SELECT {EVENT_COLUMNS} FROM mod_event WHERE grp = ?1
             AND subject IN (SELECT value FROM json_each(?2)) AND kind NOT IN ('report', 'tag')
             AND seq {op} ?3 ORDER BY seq {order} LIMIT ?4"
        );
        let mut q = conn.prepare(&sql)?;
        let subjects = json!(subjects).to_string();
        let rows = q.query_map(params![group, subjects, after, limit], event_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether `subject` in `group`'s queue bears what its subject may
    /// appeal: a label written and not since taken back, or an ejection.
    pub fn appealable(&self, group: &str, subject: &str) -> Result<bool> {
        let conn = self.conn();
        let sql = "SELECT EXISTS (SELECT 1 FROM mod_event e WHERE e.grp = ?1 AND e.subject = ?2
             AND (e.kind = 'eject' OR (e.kind = 'label' AND NOT EXISTS (SELECT 1 FROM mod_event n
                 WHERE n.grp = e.grp AND n.subject = e.subject AND n.kind = 'negate'
                 AND n.uri = e.uri AND n.seq > e.seq))))";
        Ok(conn.query_row(sql, params![group, subject], |r| r.get(0))?)
    }
}

fn forbidden(message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::FORBIDDEN, "Forbidden", message)
}

fn not_found() -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_REQUEST,
        "SubjectNotFound",
        "the subject has never been reported",
    )
}

fn invalid_subject(message: &str) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidSubject", message)
}

/// The space a record's URI is in, `at:space/<authority>/<type>/<skey>`,
/// and its authority, for a URI of a record inside one, each of its parts
/// what it says: so a URI anyone sends is bounded, as its parts are.
pub fn space_of(uri: &str) -> Option<(String, String)> {
    let record = SpaceRecordUri::parse(uri).ok()?;
    Some((record.space.to_string(), record.space.authority))
}

/// The account a public record's URI is in, `at://<did>/<collection>/<rkey>`,
/// a record in no space, each of its parts what it says.
fn public_of(uri: &str) -> Option<&str> {
    let parts: Vec<&str> = uri.strip_prefix("at://")?.split('/').collect();
    let [did, collection, rkey] = parts[..] else {
        return None;
    };
    let valid = syntax::is_did(did) && syntax::is_nsid(collection) && syntax::is_record_key(rkey);
    valid.then_some(did)
}

/// Refuses a record subject's URI that is neither in one of `group`'s
/// spaces nor in a public repo.
fn record_in(group: &str, uri: &str) -> Result<(), XrpcError> {
    let ours = space_of(uri).is_some_and(|(_, authority)| authority == group);
    if !ours && public_of(uri).is_none() {
        return Err(invalid_subject(
            "not a record in one of this group's spaces, nor in a public repo",
        ));
    }
    Ok(())
}

/// Refuses a subject that is neither an account's DID nor a record's URI
/// in one of `group`'s spaces or in a public repo.
fn subject_of(group: &str, subject: &str) -> Result<(), XrpcError> {
    match syntax::is_did(subject) {
        true => Ok(()),
        false => record_in(group, subject),
    }
}

/// A subject as the queue keys it, from the `subject` a caller sends, and
/// the version of a record it names in `cid`: an account has none.
fn subject_in(group: &str, asked: &Value) -> Result<(String, Option<String>), XrpcError> {
    let subject = asked["subject"].as_str().expect("its lexicon requires it");
    subject_of(group, subject)?;
    let cid = asked["cid"].as_str().filter(|_| !syntax::is_did(subject));
    Ok((subject.to_string(), cid.map(String::from)))
}

/// Whether `subject` in `group` is `me`'s: my account, or a record I
/// wrote, in my public repo or in one of the group's spaces.
fn owns(group: &str, me: &str, subject: &str) -> bool {
    let in_space = space_of(subject).is_some_and(|(_, authority)| authority == group);
    subject == me || public_of(subject) == Some(me) || (in_space && account_of(subject) == me)
}

impl Host {
    /// Refuses a label among `writes` in `group`'s `space` whose subject is
    /// neither an account nor a record in that same space, nor, in the
    /// members space, a member's public record: a label sits beside what
    /// it is about, so exactly those who read the one read the other.
    pub(crate) fn beside(
        &self,
        group: &str,
        space: &str,
        writes: &[Write],
    ) -> Result<(), XrpcError> {
        let members = space == members_space(group);
        for write in writes {
            let (collection, _) = write.path();
            let Some(value) = write.value().filter(|_| collection == LABEL) else {
                continue;
            };
            let subject = value["uri"].as_str().unwrap_or_default();
            let here = space_of(subject).is_some_and(|(s, _)| s == space);
            if here || syntax::is_did(subject) {
                continue;
            }
            // A public record's author's roles, in the members space alone.
            let roles = match public_of(subject).filter(|_| members) {
                Some(did) => self.roles(group, did).map_err(XrpcError::internal)?,
                None => Vec::new(),
            };
            if roles.is_empty() {
                return Err(invalid(
                    "a label's uri is an account's DID, a record's in the space it is written in, \
                     or in the members space a member's public record",
                ));
            }
        }
        Ok(())
    }
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
        let me = signed_in(self, headers, lxm).await?;
        group_of(self, group)?;
        if !self.may_do(group, &me, action)? {
            return Err(forbidden(format!("missing {action}")));
        }
        Ok(me)
    }

    /// The labels among `writes` that delete one, as they are before the
    /// writes are made in `space`, by key: what a deletion takes back.
    pub(crate) fn labels_going(
        &self,
        group: &str,
        space: &str,
        writes: &[Write],
    ) -> Result<Vec<(String, Value)>, XrpcError> {
        let mut out = Vec::new();
        for write in writes {
            let (collection, rkey) = write.path();
            if collection != LABEL || !matches!(write, Write::Delete { .. }) {
                continue;
            }
            let found = self.group_record(group, space, LABEL, rkey);
            if let Some(label) = found.map_err(XrpcError::internal)? {
                out.push((rkey.to_string(), label));
            }
        }
        Ok(out)
    }

    /// Notes each label among `made` in `group`'s queue, as `member` wrote
    /// it in `space`: a label written, or one taken back by deleting it,
    /// as `going` had it.
    pub(crate) fn labelled(
        &self,
        group: &str,
        space: &str,
        member: &str,
        made: &[Write],
        going: &[(String, Value)],
    ) -> Result<(), XrpcError> {
        // A deletion is of a label there: one where there is none is left
        // out of what is made.
        let labels = made.iter().filter(|w| w.path().0 == LABEL).filter_map(|w| {
            let gone = going.iter().find(|(k, _)| k == w.path().1);
            Some((w, w.value().or(gone.map(|(_, l)| l))?))
        });
        for (write, label) in labels {
            let kind = match write {
                Write::Delete { .. } => "negate",
                _ => "label",
            };
            let rkey = write.path().1;
            let text = |key: &str| label[key].as_str().map(String::from);
            let event = Event {
                kind: kind.into(),
                actor: Some(member.into()),
                cid: text("cid"),
                val: text("val"),
                uri: Some(format!("{space}/{group}/{LABEL}/{rkey}")),
                created_at: now_millis(),
                ..Event::default()
            };
            let subject = text("uri").unwrap_or_default();
            let kept = self.store.moderate(group, &subject, &event);
            kept.map_err(XrpcError::internal)?;
        }
        Ok(())
    }
}

/// `value`, an object, without the fields it has no value for.
fn present(mut value: Value) -> Value {
    let fields = value.as_object_mut().expect("an object");
    fields.retain(|_, v| !v.is_null());
    value
}

/// The account a subject is, or whose record it is: a record's repo, as
/// the queue keeps it, in a space or a public repo.
fn account_of(key: &str) -> &str {
    // One in a space's author comes after the space; a public record's
    // repo first.
    if let Some(path) = key.strip_prefix(SPACE_URI_PREFIX) {
        return path.split('/').nth(3).expect("a record in a space");
    }
    let public = key.strip_prefix("at://");
    public.map_or(key, |path| path.split('/').next().unwrap_or(path))
}

/// A subject as the queue answers it (`mod.defs#subjectView`), with
/// `actors`, the people it names as a view names them.
fn subject_view(s: &Subject, actors: &HashMap<String, Value>) -> Value {
    json!({
        "subject": s.subject,
        "actor": actors[account_of(&s.subject)],
        "status": s.status,
        "reportCount": s.reports,
        "tags": s.tags,
        "createdAt": iso(s.created_at),
        "updatedAt": iso(s.updated_at),
    })
}

/// Tags as an event lists them: none, when there are none.
fn tags_of(tags: &[String]) -> Option<&[String]> {
    (!tags.is_empty()).then_some(tags)
}

/// An event on `subject` as a history answers it (`mod.defs#eventView`):
/// one of the kinds the union holds, with who did it as `actors` names
/// them. A negation is the label taken back. Without `actors`, the event
/// as its subject sees it: without who did it, nor any note.
fn event_view(subject: &str, e: &Event, actors: Option<&HashMap<String, Value>>) -> Value {
    let note = e.note.as_ref().filter(|_| actors.is_some());
    let event = match e.kind.as_str() {
        "report" => json!({
            "$type": format!("{DEFS}#reportEvent"),
            "reasonType": e.reason_type,
            "reason": e.reason,
            "rule": e.rule,
        }),
        "appeal" => json!({
            "$type": format!("{DEFS}#appealEvent"),
            "note": note,
        }),
        "status" => json!({
            "$type": format!("{DEFS}#statusEvent"),
            "status": e.status,
            "note": note,
        }),
        "tag" => json!({
            "$type": format!("{DEFS}#tagEvent"),
            "add": tags_of(&e.add),
            "remove": tags_of(&e.remove),
        }),
        "label" | "negate" => json!({
            "$type": format!("{DEFS}#labelEvent"),
            "uri": e.uri,
            "val": e.val,
            "deleted": e.kind == "negate",
        }),
        _ => json!({"$type": format!("{DEFS}#ejectEvent")}),
    };
    let by = actors.and(e.actor.as_ref());
    present(json!({
        "eventId": e.seq.to_string(),
        "createdAt": iso(e.created_at),
        "subject": subject,
        "cid": e.cid,
        "createdBy": by,
        "creator": by.zip(actors).map(|(did, actors)| &actors[did]),
        "event": present(event),
    }))
}

/// The order `sort` asks for among `known`, the first unless it says.
fn sort_of(p: &Params, known: &[&'static str]) -> Result<&'static str, XrpcError> {
    let Some(asked) = p.get("sort") else {
        return Ok(known[0]);
    };
    let found = known.iter().find(|k| **k == asked.as_str()).copied();
    found.ok_or_else(|| invalid(format!("sort is one of {}", known.join(", "))))
}

/// The event a page of a history starts after, as its cursor names it.
fn after_event(p: &Params) -> Result<Option<i64>, XrpcError> {
    let cursor = p.get("cursor").map(|c| c.parse::<i64>());
    cursor.transpose().map_err(|_| invalid_cursor())
}

/// The group's queue, of a status if asked, with any of the tags asked,
/// in the order asked (the oldest first unless it says), for one holding
/// `mod.read`, or an app signed in as the group with its `rpc:` scope.
pub async fn query_subjects(
    State(host): State<Arc<Host>>,
    Query(pairs): Query<Vec<(String, String)>>,
    headers: HeaderMap,
) -> Result<Json<Value>, Refusal> {
    let p: Params = pairs.iter().cloned().collect();
    let lxm = format!("{NS}.mod.querySubjects");
    let asked = p.get("group").map(String::as_str);
    let (group, _) = doing(&host, &headers, "GET", &lxm, asked, "mod.read").await?;
    let limit = page(&p, 50, 100)?;
    let sort = sort_of(&p, &["oldest", "newest", "updated"])?;
    let cursor = p
        .get("cursor")
        .map(|c| c.split_once(':').unwrap_or((c, "")));
    let after = match cursor {
        None => None,
        Some((at, subject)) => {
            let at = at.parse::<i64>().map_err(|_| invalid_cursor())?;
            Some((at, subject))
        }
    };
    let tags: Vec<String> = pairs
        .into_iter()
        .filter_map(|(k, v)| (k == "tags").then_some(v))
        .collect();
    let status = p.get("status").map(String::as_str);
    let subjects = host
        .store
        .subjects(&group, status, &tags, sort, after, limit)
        .map_err(XrpcError::internal)?;
    let actors = host.actor_views(subjects.iter().map(|s| account_of(&s.subject)));
    let actors = actors.await;
    let views: Vec<Value> = subjects.iter().map(|s| subject_view(s, &actors)).collect();
    let mut out = json!({ "subjects": views });
    if subjects.len() == limit as usize
        && let Some(last) = subjects.last()
    {
        let at = match sort {
            "updated" => last.updated_at,
            _ => last.created_at,
        };
        out["cursor"] = json!(format!("{at}:{}", last.subject));
    }
    Ok(Json(out))
}

/// What happened to a subject in the group's queue, the newest first
/// unless asked, a page at a time; and a record subject itself, as a
/// moderator reviews it. For one holding `mod.read`, or an app signed in
/// as the group with its `rpc:` scope, which sees the record as its
/// member would.
pub async fn query_subject_history(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Json<Value>, Refusal> {
    let lxm = format!("{NS}.mod.querySubjectHistory");
    let asked = p.get("group").map(String::as_str);
    let (group, me) = doing(&host, &headers, "GET", &lxm, asked, "mod.read").await?;
    let subject = param(&p, "subject")?;
    subject_of(&group, subject)?;
    let limit = page(&p, 50, 100)?;
    let newest = sort_of(&p, &["newest", "oldest"])? == "newest";
    let after = after_event(&p)?;
    let cid = p.get("cid").map(String::as_str);
    let events = host
        .store
        .events(&group, subject, cid, newest, after, limit)
        .map_err(XrpcError::internal)?;
    let actors = host.actor_views(events.iter().filter_map(|e| e.actor.as_deref()));
    let actors = actors.await;
    let view = |e: &Event| event_view(subject, e, Some(&actors));
    let mut out = json!({ "events": events.iter().map(view).collect::<Vec<_>>() });
    if events.len() == limit as usize
        && let Some(last) = events.last()
    {
        out["cursor"] = json!(last.seq.to_string());
    }
    let reviewed = host.reviewed(&group, subject, &me).await;
    if let Some(r) = reviewed.map_err(XrpcError::internal)? {
        out["record"] = json!({
            "uri": subject,
            "cid": r.record.cid.to_string(),
            "value": value_of(&r),
            "createdAt": iso(r.record.indexed_at),
            "takenDown": r.taken_down,
        });
    }
    Ok(Json(out))
}

/// A reviewed record's value, as JSON.
fn value_of(r: &Reviewed) -> Value {
    let value = drisl::decode(&r.record.value).expect("a record the host indexed");
    value.to_json()
}

/// A file a record subject holds, for a moderator reviewing it, whom its
/// space lets read it: served though the group took the record down.
pub async fn get_mod_subject_blob(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, XrpcError> {
    let group = param(&p, "group")?;
    let me = host
        .moderator(
            &headers,
            &format!("{INTERNAL}.getModSubjectBlob"),
            group,
            "mod.read",
        )
        .await?;
    let subject = param(&p, "subject")?;
    subject_of(group, subject)?;
    let cid = Cid::parse(param(&p, "cid")?).map_err(|e| invalid(format!("cid: {e}")))?;
    let gone = || XrpcError::new(StatusCode::BAD_REQUEST, "NotFound", "no such file here");
    let reviewed = host.reviewed(group, subject, &me).await;
    let reviewed = reviewed.map_err(XrpcError::internal)?;
    let holds = |r: &Reviewed| blob_refs(&value_of(r)).contains(&cid);
    let r = reviewed.filter(holds).ok_or_else(gone)?;
    let fetched = host.space_blob(group, &r.space, &r.record.did, &cid).await;
    let (mime, bytes) = fetched.ok_or_else(gone)?;
    Ok(served(mime, bytes))
}

/// The tags `body` lists under `key`, each in label value syntax.
fn tags_in(body: &Value, key: &str) -> Result<Vec<String>, XrpcError> {
    let listed = body[key].as_array().map(Vec::as_slice).unwrap_or_default();
    let tags = listed.iter().map(|t| t.as_str().expect("its lexicon says"));
    if let Some(bad) = tags.clone().find(|t| !is_own_label(t)) {
        return Err(invalid(format!(
            "{key}: {bad} is not lowercase letters and hyphens"
        )));
    }
    Ok(tags.map(String::from).collect())
}

/// Changes a subject: its status (reopening it, or resolving it), a note
/// beside it, its tags, each only if asked. A status or a note is a
/// status event, a status unasked the one it has; tags, a tag event. For
/// one holding `mod.resolve`, or an app signed in as the group with its
/// `rpc:` scope, as its member.
pub async fn update_subject(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Refusal> {
    let lxm = format!("{NS}.mod.updateSubject");
    let asked = body["group"].as_str();
    let (group, me) = doing(&host, &headers, "POST", &lxm, asked, "mod.resolve").await?;
    let (subject, cid) = subject_in(&group, &body)?;
    let add = tags_in(&body, "addTags")?;
    let remove = tags_in(&body, "removeTags")?;
    let found = host.store.subject(&group, &subject);
    let found = found.map_err(XrpcError::internal)?.ok_or_else(not_found)?;
    let now = now_millis();
    let at = Event {
        actor: Some(me),
        cid,
        created_at: now,
        ..Event::default()
    };
    let status = body["status"].as_str();
    let note = body["note"].as_str();
    let mut events = Vec::new();
    if status.is_some() || note.is_some() {
        events.push(Event {
            kind: "status".into(),
            status: Some(status.unwrap_or(found.status.as_str()).into()),
            note: note.map(String::from),
            ..at.clone()
        });
    }
    if !add.is_empty() || !remove.is_empty() {
        events.push(Event {
            kind: "tag".into(),
            add,
            remove,
            ..at
        });
    }
    for event in &events {
        let kept = host.store.moderate(&group, &subject, event);
        kept.map_err(XrpcError::internal)?;
    }
    let now = host.store.subject(&group, &subject);
    let now = now.map_err(XrpcError::internal)?;
    let now = now.expect("found above");
    let actors = host.actor_views([account_of(&now.subject)]).await;
    Ok(Json(subject_view(&now, &actors)))
}

/// Whether the group's labeler takes reports where it lists `key`: it
/// does unless it lists them, and lists `value` among them.
fn accepts(labeler: &Value, key: &str, value: &str) -> bool {
    labeler[key]
        .as_array()
        .is_none_or(|listed| listed.iter().any(|v| v == value))
}

/// A report, from one who can read its subject: an account in a group
/// here, a record in one of its spaces, or a member's public record, into
/// that group's queue.
pub async fn submit_report(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.mod.submitReport")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    let reason_type = body["reasonType"]
        .as_str()
        .expect("its lexicon requires it");
    // A rule is cited by its URI, a record's in the group's members space.
    let rule = body["rule"].as_str();
    let members = members_space(group);
    if rule.is_some_and(|r| space_of(r).is_none_or(|(space, _)| space != members)) {
        return Err(invalid("rule is a rule in the group's members space"));
    }
    let (subject, cid) = subject_in(group, &body)?;
    let labeler = host.labeler_of(group)?;
    if !accepts(&labeler, "reasonTypes", reason_type) {
        return Err(invalid(format!(
            "the group takes no reports of {reason_type}"
        )));
    }
    // A record in a space is reported in its own; an account, or a
    // member's public record, in the members space, where its labels go.
    let collection =
        (!syntax::is_did(&subject)).then(|| subject.rsplit('/').nth(1).expect("a record's URI"));
    let kind = if collection.is_some() {
        "record"
    } else {
        "account"
    };
    let space = match space_of(&subject) {
        Some((space, _)) => space,
        None => {
            let account = public_of(&subject).unwrap_or(&subject);
            let roles = host.roles(group, account).map_err(XrpcError::internal)?;
            if roles.is_empty() {
                return Err(invalid_subject("not a member of this group"));
            }
            members
        }
    };
    let taken = accepts(&labeler, "subjectTypes", kind)
        && collection.is_none_or(|c| accepts(&labeler, "subjectCollections", c));
    if !taken {
        return Err(invalid_subject("the group takes no reports of this kind"));
    }
    let hosted = host.store.space(&space).map_err(XrpcError::internal)?;
    let hosted = hosted.ok_or_else(|| invalid_subject("not in a space hosted here"))?;
    let reads = host.may(&space, &hosted, Some(me.as_str()), Access::Read);
    if !reads.map_err(XrpcError::internal)? {
        return Err(invalid_subject("not in a space you can read"));
    }
    // Counted once it goes in the queue: a refused one fills nothing.
    host.spend(&crate::limits::REPORTS, &me, 1)?;
    let now = now_millis();
    let event = Event {
        kind: "report".into(),
        actor: Some(me),
        cid,
        reason_type: Some(reason_type.into()),
        reason: body["reason"].as_str().map(String::from),
        rule: rule.map(String::from),
        created_at: now,
        ..Event::default()
    };
    let id = host.store.moderate(group, &subject, &event);
    let id = id.map_err(XrpcError::internal)?;
    Ok(Json(
        json!({"eventId": id.to_string(), "createdAt": iso(now)}),
    ))
}

/// An appeal of a label or an ejection, by the account it is on or the
/// author of the record it is on, though they have left the group since:
/// it reopens the subject in the group's queue.
pub async fn submit_appeal(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.mod.submitAppeal")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    let subject = body["subject"].as_str().expect("its lexicon requires it");
    if !owns(group, &me, subject) {
        return Err(invalid_subject("not your account, nor a record of yours"));
    }
    let appealable = host.store.appealable(group, subject);
    if !appealable.map_err(XrpcError::internal)? {
        return Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "NotAppealable",
            "the group has neither labelled nor ejected it",
        ));
    }
    // Counted as a report is: each asks the group's moderators to look.
    host.spend(&crate::limits::REPORTS, &me, 1)?;
    let now = now_millis();
    let event = Event {
        kind: "appeal".into(),
        actor: Some(me),
        note: body["note"].as_str().map(String::from),
        created_at: now,
        ..Event::default()
    };
    let id = host.store.moderate(group, subject, &event);
    let id = id.map_err(XrpcError::internal)?;
    Ok(Json(
        json!({"eventId": id.to_string(), "createdAt": iso(now)}),
    ))
}

/// What the group did to the caller's account and records, or to the one
/// of them asked, the newest first unless asked, a page at a time: no
/// report, tag or note, nor who did it. Theirs to read though they have
/// left the group.
pub async fn query_self_history(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.mod.querySelfHistory")).await?;
    let group = param(&p, "group")?;
    group_of(&host, group)?;
    let subjects = match p.get("subject") {
        None => host
            .store
            .subjects_of(group, &me)
            .map_err(XrpcError::internal)?,
        Some(s) if owns(group, &me, s) => vec![s.clone()],
        Some(_) => return Err(invalid_subject("not your account, nor a record of yours")),
    };
    let limit = page(&p, 50, 100)?;
    let newest = sort_of(&p, &["newest", "oldest"])? == "newest";
    let after = after_event(&p)?;
    let events = host
        .store
        .own_events(group, &subjects, newest, after, limit);
    let events = events.map_err(XrpcError::internal)?;
    let views: Vec<Value> = events.iter().map(|(s, e)| event_view(s, e, None)).collect();
    let mut out = json!({ "events": views });
    if events.len() == limit as usize
        && let Some((_, last)) = events.last()
    {
        out["cursor"] = json!(last.seq.to_string());
    }
    Ok(Json(out))
}

/// A label a group defines for itself, or a tag a moderator gives a
/// subject: lowercase letters and hyphens, as atproto has a labeler's own
/// values; never one of the values atproto defines, which start with `!`.
fn is_own_label(label: &str) -> bool {
    !label.is_empty() && label.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

impl Host {
    /// The group's labeler, as its members space keeps it, and its
    /// policies' values and definitions, made lists where it had none.
    fn labeler_of(&self, group: &str) -> Result<Value, XrpcError> {
        let members = members_space(group);
        let kept = self.group_record(group, &members, &format!("{NS}.labeler"), "self");
        let mut labeler = kept.map_err(XrpcError::internal)?.unwrap_or_default();
        for key in ["labelValues", "labelValueDefinitions"] {
            if !labeler["policies"][key].is_array() {
                labeler["policies"][key] = json!([]);
            }
        }
        Ok(labeler)
    }

    /// `labeler` put back whole, as the group, after `change` to its
    /// policies' values and definitions.
    async fn relabel(
        &self,
        group: &str,
        mut labeler: Value,
        change: impl FnOnce(&mut Vec<Value>, &mut Vec<Value>),
    ) -> Result<(), XrpcError> {
        let policies = labeler["policies"].as_object_mut().expect("made an object");
        let mut values = policies["labelValues"].take();
        let mut definitions = policies["labelValueDefinitions"].take();
        change(
            values.as_array_mut().expect("made a list"),
            definitions.as_array_mut().expect("made a list"),
        );
        policies.insert("labelValues".into(), values);
        policies.insert("labelValueDefinitions".into(), definitions);
        let write = Write::Update {
            collection: format!("{NS}.labeler"),
            rkey: "self".into(),
            value: labeler,
        };
        self.space_write(group, &members_space(group), &[write])
            .await?;
        Ok(())
    }
}

/// Defines one of the group's own labels, or changes one it defines, in
/// its labeler, the label among the values it applies. A change replaces
/// how apps show it but only the name and description in the given
/// language: the definition keeps its other languages, in their order,
/// and a new language goes after them.
pub async fn put_label_definition(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"].as_str().expect("its lexicon requires it");
    host.moderator(
        &headers,
        &format!("{INTERNAL}.putLabelDefinition"),
        group,
        "group.configure",
    )
    .await?;
    let label = body["label"].as_str().expect("its lexicon requires it");
    if !is_own_label(label) {
        return Err(invalid("label is lowercase letters and hyphens"));
    }
    let or = |key: &str, unless: &str| body[key].as_str().unwrap_or(unless).to_string();
    let lang = or("lang", "en");
    let labeler = host.labeler_of(group)?;
    let mut locales: Vec<Value> = labeler["policies"]["labelValueDefinitions"]
        .as_array()
        .and_then(|ds| ds.iter().find(|d| d["identifier"] == label))
        .and_then(|d| d["locales"].as_array().cloned())
        .unwrap_or_default();
    let locale = json!({"lang": lang, "name": body["name"], "description": body["description"]});
    match locales.iter_mut().find(|l| l["lang"] == lang.as_str()) {
        Some(same) => *same = locale,
        None => locales.push(locale),
    }
    let mut definition = json!({
        "identifier": label,
        "severity": or("severity", "inform"),
        "blurs": or("blurs", "none"),
        "defaultSetting": or("defaultSetting", "warn"),
        "locales": locales,
    });
    if body["adultOnly"] == true {
        definition["adultOnly"] = json!(true);
    }
    let defined = definition.clone();
    let change = |values: &mut Vec<Value>, definitions: &mut Vec<Value>| {
        if !values.contains(&json!(label)) {
            values.push(json!(label));
        }
        definitions.retain(|d| d["identifier"] != label);
        definitions.push(defined);
    };
    host.relabel(group, labeler, change).await?;
    Ok(Json(definition))
}

/// Stops defining one of the group's own labels: its definition, and the
/// value among those it applies. Labels already applied stay.
pub async fn delete_label_definition(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"].as_str().expect("its lexicon requires it");
    host.moderator(
        &headers,
        &format!("{INTERNAL}.deleteLabelDefinition"),
        group,
        "group.configure",
    )
    .await?;
    let label = body["label"].as_str().expect("its lexicon requires it");
    if !is_own_label(label) {
        return Err(invalid("label is lowercase letters and hyphens"));
    }
    let labeler = host.labeler_of(group)?;
    let change = |values: &mut Vec<Value>, definitions: &mut Vec<Value>| {
        values.retain(|v| v != label);
        definitions.retain(|d| d["identifier"] != label);
    };
    host.relabel(group, labeler, change).await?;
    Ok(Json(json!({})))
}
