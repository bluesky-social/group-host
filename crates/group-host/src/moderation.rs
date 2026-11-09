//! Moderation, a group's own: reports in, labels out, a queue between.
//!
//! - **Labels** are `com.atproto.label.label` records the group writes,
//!   as any record of its own, beside their subject: a record's in the
//!   space it is in, an account's in the members space by custom. Whoever
//!   reads the content reads its labels. Deleting a label takes it back;
//!   the queue keeps that it was written, and by whom.
//! - **The queue** (subjects, their status, what happened to each) is the
//!   host's to keep, not the group's to publish.
//! - **Reports** come through `fyi.opensocial.createReport`, from whoever
//!   can read what they report: an account in the group, or a record in
//!   one of its spaces here.

use crate::Host;
use crate::api::{Params, invalid, page, param};
use crate::authority::Access;
use crate::group::{members_space, meta_space};
use crate::group_api::{group_of, signed_in};
use crate::rbac::{LABEL, NS};
use crate::repo::Write;
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
use std::collections::HashMap;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// An account, as a subject is sent and answered.
const ACCOUNT: &str = "fyi.opensocial.defs#accountSubject";
/// A record, as a subject is sent and answered.
const RECORD: &str = "fyi.opensocial.defs#recordSubject";

/// A subject in a group's queue: an account's DID, or a record's URI.
#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub subject: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub reports: i64,
}

/// Something that happened to a subject: a `report`, a `status` set, a
/// `label` written or taken back (`negate`), or an `eject`ion.
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
    pub created_at: i64,
}

const SUBJECT_COLUMNS: &str = "s.subject, s.status, s.created_at, s.updated_at, (SELECT count(*) FROM mod_event e WHERE e.grp = s.grp AND e.subject = s.subject AND e.kind = 'report')";

fn subject_row(row: &Row) -> rusqlite::Result<Subject> {
    Ok(Subject {
        subject: row.get(0)?,
        status: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        reports: row.get(4)?,
    })
}

fn event_row(row: &Row) -> rusqlite::Result<Event> {
    Ok(Event {
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
    })
}

impl Store {
    /// Keeps `event` on `subject` in `group`'s queue, opening the subject
    /// if it is new. A status set is the subject's; a report reopens one
    /// resolved, as someone asks again.
    pub fn moderate(&self, group: &str, subject: &str, event: &Event) -> Result<i64> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let e = event;
        let reopen = e.kind == "report";
        tx.execute(
            "INSERT INTO mod_subject (grp, subject, status, created_at, updated_at) VALUES (?1, ?2, 'open', ?3, ?3)
             ON CONFLICT (grp, subject) DO UPDATE SET updated_at = ?3,
             status = coalesce(?5, CASE WHEN ?4 AND status = 'resolved' THEN 'open' ELSE status END)",
            params![group, subject, e.created_at, reopen, e.status],
        )?;
        tx.execute(
            "INSERT INTO mod_event (grp, subject, kind, actor, cid, reason_type, reason, rule, status, val, uri, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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

    /// `group`'s queue, of `status`, the latest changed first, after
    /// `before` (an `updated_at` and subject), at most `limit`.
    pub fn subjects(
        &self,
        group: &str,
        status: &str,
        before: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<Vec<Subject>> {
        let conn = self.conn();
        let (at, after) = before.unwrap_or((i64::MAX, ""));
        let sql = format!(
            "SELECT {SUBJECT_COLUMNS} FROM mod_subject s WHERE s.grp = ?1 AND s.status = ?2
             AND (s.updated_at < ?3 OR (s.updated_at = ?3 AND s.subject > ?4))
             ORDER BY s.updated_at DESC, s.subject LIMIT ?5"
        );
        let mut q = conn.prepare(&sql)?;
        let rows = q.query_map(params![group, status, at, after, limit], subject_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What happened to `subject` in `group`'s queue, in order: of the
    /// version `cid` alone if named, after the event `after`, at most
    /// `limit`.
    pub fn events(
        &self,
        group: &str,
        subject: &str,
        cid: Option<&str>,
        after: i64,
        limit: u32,
    ) -> Result<Vec<Event>> {
        let conn = self.conn();
        let sql = "SELECT seq, kind, actor, cid, reason_type, reason, rule, status, val, uri, note, created_at
             FROM mod_event WHERE grp = ?1 AND subject = ?2 AND (?3 IS NULL OR cid = ?3) AND seq > ?4
             ORDER BY seq LIMIT ?5";
        let mut q = conn.prepare(sql)?;
        let rows = q.query_map(params![group, subject, cid, after, limit], event_row)?;
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
        "the subject has never been reported",
    )
}

fn invalid_subject(message: &str) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, "InvalidSubject", message)
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

/// The longest reason type a report gives: one of the known ones, or
/// another as long.
const REASON_TYPE_MAX: usize = 256;

/// The space a record subject's URI is in, one of `group`'s.
fn in_group(group: &str, uri: &str) -> Result<String, XrpcError> {
    let space = space_of(uri).filter(|(_, authority)| authority == group);
    let space = space.ok_or_else(|| invalid_subject("not a record in one of this group's spaces"));
    Ok(space?.0)
}

/// A subject as the queue keys it (an account's DID, or a record's URI in
/// one of `group`'s spaces) and the version of a record it names, from
/// the union a caller sends.
fn subject_in(group: &str, asked: &Value) -> Result<(String, Option<String>), XrpcError> {
    let field = |key: &str| asked[key].as_str().map(String::from);
    match asked["$type"].as_str() {
        Some(ACCOUNT) => Ok((field("did").expect("its lexicon requires it"), None)),
        Some(RECORD) => {
            let uri = field("uri").expect("its lexicon requires it");
            in_group(group, &uri)?;
            Ok((uri, field("cid")))
        }
        _ => Err(invalid_subject("a subject is an account or a record")),
    }
}

/// A subject as the queue keys it, answered as the union it was sent as.
fn subject_ref(key: &str) -> Value {
    if key.starts_with("at://") {
        json!({"$type": RECORD, "uri": key})
    } else {
        json!({"$type": ACCOUNT, "did": key})
    }
}

/// Refuses a label among `writes` in `space` whose subject is neither an
/// account nor a record in that same space: a label sits beside what it
/// is about, so exactly those who read the one read the other.
pub(crate) fn beside(space: &str, writes: &[Write]) -> Result<(), XrpcError> {
    for write in writes {
        let (collection, _) = write.path();
        let Some(value) = write.value().filter(|_| collection == LABEL) else {
            continue;
        };
        let subject = value["uri"].as_str().unwrap_or_default();
        let here = space_of(subject).is_some_and(|(s, _)| s == space);
        if !here && !syntax::is_did(subject) {
            return Err(invalid(
                "a label's uri is an account's DID, or a record's in the space it is written in",
            ));
        }
    }
    Ok(())
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
/// the queue keeps it, in a space.
fn account_of<'a>(key: &'a str) -> &'a str {
    let repo = |path: &'a str| path.split('/').nth(4).expect("a record in a space");
    key.strip_prefix("at://").map_or(key, repo)
}

/// A subject as the queue answers it, with `actors`, the people it names
/// as a view names them.
fn subject_view(s: &Subject, actors: &HashMap<String, Value>) -> Value {
    json!({
        "subject": subject_ref(&s.subject),
        "actor": actors[account_of(&s.subject)],
        "status": s.status,
        "reportCount": s.reports,
        "createdAt": iso(s.created_at),
        "updatedAt": iso(s.updated_at),
    })
}

/// An event as the history answers it: one of the four kinds the union
/// holds, with who did it as `actors` names them. A negation is the label
/// taken back.
fn event_view(e: &Event, actors: &HashMap<String, Value>) -> Value {
    let event = match e.kind.as_str() {
        "report" => json!({
            "$type": "fyi.opensocial.defs#reportEvent",
            "reasonType": e.reason_type,
            "reason": e.reason,
            "rule": e.rule,
        }),
        "status" => json!({
            "$type": "fyi.opensocial.defs#statusEvent",
            "status": e.status,
            "note": e.note,
        }),
        "label" | "negate" => json!({
            "$type": "fyi.opensocial.defs#labelEvent",
            "uri": e.uri,
            "val": e.val,
            "deleted": e.kind == "negate",
        }),
        _ => json!({"$type": "fyi.opensocial.defs#ejectEvent"}),
    };
    present(json!({
        "id": e.seq.to_string(),
        "createdAt": iso(e.created_at),
        "createdBy": e.actor,
        "creator": e.actor.as_ref().map(|did| &actors[did]),
        "cid": e.cid,
        "event": present(event),
    }))
}

/// The group's queue, of a status (`open` unless asked), the latest
/// changed first.
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
    let status = p.get("status").map_or("open", String::as_str);
    let subjects = host
        .store
        .subjects(group, status, before, limit)
        .map_err(XrpcError::internal)?;
    let actors = host.actor_views(subjects.iter().map(|s| account_of(&s.subject)));
    let actors = actors.await;
    let views: Vec<Value> = subjects.iter().map(|s| subject_view(s, &actors)).collect();
    let mut out = json!({ "subjects": views });
    if subjects.len() == limit as usize
        && let Some(last) = subjects.last()
    {
        out["cursor"] = json!(format!("{}:{}", last.updated_at, last.subject));
    }
    Ok(Json(out))
}

/// What happened to a subject in the group's queue, oldest first, a page
/// at a time.
pub async fn get_subject_history(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let group = param(&p, "group")?;
    host.moderator(&headers, "getSubjectHistory", group, "mod.read")
        .await?;
    let subject = param(&p, "subject")?;
    if !syntax::is_did(subject) {
        in_group(group, subject)?;
    }
    let limit = page(&p, 50, 100)?;
    let after = match p.get("cursor") {
        None => 0,
        Some(c) => c
            .parse::<i64>()
            .map_err(|_| invalid("a cursor this host gave"))?,
    };
    let cid = p.get("cid").map(String::as_str);
    let events = host
        .store
        .events(group, subject, cid, after, limit)
        .map_err(XrpcError::internal)?;
    let actors = host.actor_views(events.iter().filter_map(|e| e.actor.as_deref()));
    let actors = actors.await;
    let views: Vec<Value> = events.iter().map(|e| event_view(e, &actors)).collect();
    let mut out = json!({ "events": views });
    if events.len() == limit as usize
        && let Some(last) = events.last()
    {
        out["cursor"] = json!(last.seq.to_string());
    }
    Ok(Json(out))
}

/// Resolves or escalates a subject, with a note if one is given; setting
/// the status it has keeps the note.
pub async fn resolve_subject(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"].as_str().expect("its lexicon requires it");
    let me = host
        .moderator(&headers, "resolveSubject", group, "mod.resolve")
        .await?;
    let (subject, cid) = subject_in(group, &body["subject"])?;
    let status = body["status"].as_str().expect("its lexicon requires it");
    let found = host.store.subject(group, &subject);
    if found.map_err(XrpcError::internal)?.is_none() {
        return Err(not_found());
    }
    let event = Event {
        kind: "status".into(),
        actor: Some(me),
        cid,
        status: Some(status.into()),
        note: body["note"].as_str().map(String::from),
        created_at: now_millis(),
        ..Event::default()
    };
    host.store
        .moderate(group, &subject, &event)
        .map_err(XrpcError::internal)?;
    let now = host.store.subject(group, &subject);
    let now = now.map_err(XrpcError::internal)?;
    let now = now.expect("just kept");
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
/// here, or a record in one of its spaces, into that group's queue.
pub async fn create_report(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.createReport")).await?;
    let group = body["group"].as_str().expect("its lexicon requires it");
    group_of(&host, group)?;
    let reason_type = body["reasonType"]
        .as_str()
        .expect("its lexicon requires it");
    if reason_type.len() > REASON_TYPE_MAX {
        return Err(invalid(format!(
            "reasonType is {REASON_TYPE_MAX} bytes at most"
        )));
    }
    // A rule is cited by its URI, a record's in the group's meta space.
    let rule = body["rule"].as_str();
    let meta = meta_space(group);
    if rule.is_some_and(|r| space_of(r).is_none_or(|(space, _)| space != meta)) {
        return Err(invalid("rule is a rule in the group's meta space"));
    }
    let (subject, cid) = subject_in(group, &body["subject"])?;
    let labeler = host.group_record(group, &meta, &format!("{NS}.labeler"), "self");
    let labeler = labeler.map_err(XrpcError::internal)?.unwrap_or_default();
    if !accepts(&labeler, "reasonTypes", reason_type) {
        return Err(invalid(format!(
            "the group takes no reports of {reason_type}"
        )));
    }
    // An account is reported in the members space, where its labels go;
    // a record, in its own.
    let (space, kind, collection) = match space_of(&subject) {
        None => {
            let roles = host.roles(group, &subject).map_err(XrpcError::internal)?;
            if roles.is_empty() {
                return Err(invalid_subject("not a member of this group"));
            }
            (members_space(group), "account", None)
        }
        Some((space, _)) => {
            let collection = subject.rsplit('/').nth(1).map(String::from);
            (space, "record", collection)
        }
    };
    let taken = accepts(&labeler, "subjectTypes", kind)
        && collection.is_none_or(|c| accepts(&labeler, "subjectCollections", &c));
    if !taken {
        return Err(invalid_subject("the group takes no reports of this kind"));
    }
    let hosted = host.store.space(&space).map_err(XrpcError::internal)?;
    let hosted = hosted.ok_or_else(|| invalid_subject("not in a space hosted here"))?;
    let reads = host.may(&space, &hosted, Some(me.as_str()), Access::Read);
    if !reads.map_err(XrpcError::internal)? {
        return Err(invalid_subject("not in a space you can read"));
    }
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
    Ok(Json(json!({"id": id.to_string(), "createdAt": iso(now)})))
}

/// A label a group defines for itself: lowercase letters and hyphens, as
/// atproto has a labeler's own values; never one of the values atproto
/// defines, which start with `!`.
fn is_own_label(label: &str) -> bool {
    !label.is_empty() && label.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

impl Host {
    /// The group's labeler, as its meta space keeps it, and its policies'
    /// values and definitions, made lists where it had none.
    fn labeler_of(&self, group: &str) -> Result<Value, XrpcError> {
        let meta = meta_space(group);
        let kept = self.group_record(group, &meta, &format!("{NS}.labeler"), "self");
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
        self.space_write(group, &meta_space(group), &[write])
            .await?;
        Ok(())
    }
}

/// Defines one of the group's own labels, or changes one it defines, in
/// its labeler, the label among the values it applies.
pub async fn put_label_definition(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let group = body["group"].as_str().expect("its lexicon requires it");
    host.moderator(&headers, "putLabelDefinition", group, "group.configure")
        .await?;
    let label = body["label"].as_str().expect("its lexicon requires it");
    if !is_own_label(label) {
        return Err(invalid("label is lowercase letters and hyphens"));
    }
    let or = |key: &str, unless: &str| body[key].as_str().unwrap_or(unless).to_string();
    let mut definition = json!({
        "identifier": label,
        "severity": or("severity", "inform"),
        "blurs": or("blurs", "none"),
        "defaultSetting": or("defaultSetting", "warn"),
        "locales": [{"lang": or("lang", "en"), "name": body["name"], "description": body["description"]}],
    });
    if body["adultOnly"] == true {
        definition["adultOnly"] = json!(true);
    }
    let labeler = host.labeler_of(group)?;
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
    host.moderator(&headers, "deleteLabelDefinition", group, "group.configure")
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
