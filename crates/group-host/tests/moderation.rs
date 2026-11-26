//! A group's moderation: reports from those who can read what they
//! report, labels written beside what they label, the queue between, and
//! appeals from those the group labelled or put out.

mod common;

use common::groups::*;
use common::oauth::{App, group_token};
use common::spaces::*;
use common::{Running, start};
use group_host::group::{REASONS, members_space, meta_space};
use group_host::moderation::{Event, space_of};
use group_host::repo::Write;
use serde_json::{Value as Json, json};
use std::time::Duration;

const CALENDAR: &str = "com.example.calendar";
const LABEL: &str = "com.atproto.label.record";
const CID: &str = "bafyreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm";
/// Where the moderation views and events are defined.
const DEFS: &str = "org.atgrp.mod.defs";

/// The report reason `name`, as its token: `Spam`, `Rude`, …
fn reason(name: &str) -> String {
    format!("{DEFS}#reason{name}")
}

async fn membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let collection = format!("{NS}.membership");
    let value = json!({"$type": collection, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
    let rkey = who.to_string();
    let exists = h
        .host
        .group_record(did, &members_space(did), &collection, &rkey)
        .unwrap()
        .is_some();
    let write = match exists {
        true => Write::Update {
            collection,
            rkey,
            value,
        },
        false => Write::Create {
            collection,
            rkey,
            value,
        },
    };
    h.host
        .space_write(did, &members_space(did), &[write])
        .await
        .unwrap();
}

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = nsid(method);
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn query(h: &Running, who: &Authority, method: &str, q: &str) -> (u16, Json) {
    let lxm = nsid(method);
    get(h, &lxm, q, &[auth(h, who, &lxm)]).await
}

/// `body` with the fields of `subject` (its `subject`, and its `cid` if
/// it names one) added.
fn about(mut body: Json, subject: &Json) -> Json {
    for (k, v) in subject.as_object().unwrap() {
        body[k] = v.clone();
    }
    body
}

async fn report(h: &Running, who: &Authority, group: &str, subject: Json) -> (u16, Json) {
    let body = json!({"group": group, "reasonType": reason("Spam"), "reason": "ads"});
    call(h, who, "mod.submitReport", about(body, &subject)).await
}

/// `who` appeals what the group did to `subject`, saying `note`.
async fn appeal(h: &Running, who: &Authority, group: &str, subject: &str) -> (u16, Json) {
    let body = json!({"group": group, "subject": subject, "note": "it was a joke"});
    call(h, who, "mod.submitAppeal", body).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// A record as a subject: by its URI alone.
fn record(uri: &str) -> Json {
    json!({"subject": uri})
}

/// A record as a subject, at the version `cid`.
fn version(uri: &str, cid: &str) -> Json {
    json!({"subject": uri, "cid": cid})
}

/// An account as a subject: its DID.
fn account(did: &str) -> Json {
    json!({"subject": did})
}

/// Each event's kind, as a history answers it: `reportEvent`, …
fn kinds(history: &Json) -> Vec<String> {
    let events = history["events"].as_array().unwrap();
    let kind = |e: &Json| {
        let kind = e["event"]["$type"].as_str().unwrap();
        kind.replace(&format!("{DEFS}#"), "")
    };
    events.iter().map(kind).collect()
}

/// The subjects a queue lists, in order.
fn listed(queue: &Json) -> Vec<String> {
    let subjects = queue["subjects"].as_array().unwrap();
    let subject = |s: &Json| s["subject"].as_str().unwrap().to_string();
    subjects.iter().map(subject).collect()
}

/// Riders' queue, as `who` reads it, with `more` asked.
async fn queue(h: &Running, who: &Authority, did: &str, more: &str) -> (u16, Json) {
    query(h, who, "mod.querySubjects", &format!("group={did}{more}")).await
}

/// `subject`'s history in Riders' queue, as `who` reviews it, with `more`
/// asked.
async fn history(
    h: &Running,
    who: &Authority,
    did: &str,
    subject: &str,
    more: &str,
) -> (u16, Json) {
    let q = format!("group={did}&subject={}{more}", urlencode(subject));
    query(h, who, "mod.querySubjectHistory", &q).await
}

/// What `who` reads of what Riders did to them, with `more` asked.
async fn own_history(h: &Running, who: &Authority, did: &str, more: &str) -> (u16, Json) {
    let q = format!("group={did}{more}");
    query(h, who, "mod.querySelfHistory", &q).await
}

/// `who` changes `subject` in Riders' queue as `change` asks.
async fn update(
    h: &Running,
    who: &Authority,
    did: &str,
    subject: &str,
    mut change: Json,
) -> (u16, Json) {
    change["group"] = json!(did);
    change["subject"] = json!(subject);
    call(h, who, "mod.updateSubject", change).await
}

/// Alice puts `member` out of Riders, deleting their membership as the
/// group.
async fn eject(h: &Running, alice: &Authority, did: &str, member: &str) {
    let app = App::new("console");
    let members = members_space(did);
    let token = app.token_for(h, alice, did, &members).await;
    let body = json!({"repo": did, "space": members, "collection": format!("{NS}.membership"), "rkey": member});
    let said = app
        .call(h, "com.atproto.space.deleteRecord", &token, Some(&body))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
}

/// A moment, so that what comes next is later by the clock.
async fn tick() {
    tokio::time::sleep(Duration::from_millis(3)).await;
}

/// Riders, Alice's, with a calendar its members read, Bob a member and
/// Carol not; and a post in the calendar, by its URI and CID.
async fn group(h: &Running) -> (String, String, Json, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let readers = json!({"$type": "org.atgrp.policy", "grants": [{"role": "member", "read": true, "write": true}]});
    let space = common::oauth::make_space(h, &alice, &did, CALENDAR, "rides", readers).await;
    membership(h, &did, &bob.did, &["member"]).await;
    let post = json!({"$type": "com.atproto.repo.strongRef", "uri": format!("{space}/{}/com.example.event/ride", bob.did), "cid": CID});
    (did, space, post, [alice, bob, carol])
}

#[tokio::test]
async fn takes_a_report_from_one_who_reads_it() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, carol]) = group(&h).await;
    // A new group's labeler, in its members space, takes every reason.
    let members = members_space(&did);
    let labeler = h
        .host
        .group_record(&did, &members, &format!("{NS}.labeler"), "self")
        .unwrap()
        .unwrap();
    assert_eq!(labeler["reasonTypes"], json!(REASONS));
    let uri = post["uri"].as_str().unwrap();
    let subject = version(uri, CID);
    let (status, said) = report(&h, &bob, &did, subject.clone()).await;
    assert_eq!(status, 200, "{said}");
    assert!(said["createdAt"].is_string());
    // Each report its own event, however close together they come.
    let (_, again) = report(&h, &bob, &did, subject.clone()).await;
    assert!(said["eventId"].is_string());
    assert_ne!(said["eventId"], again["eventId"]);
    let (_, open) = queue(&h, &alice, &did, "").await;
    // A subject as its URI, or an account's DID, as it was sent.
    assert_eq!(open["subjects"][0]["subject"], uri);
    assert_eq!(open["subjects"][0]["status"], "open");
    assert_eq!(open["subjects"][0]["reportCount"], 2);
    assert_eq!(open["subjects"][0]["tags"], json!([]));
    // Whose record it is, as a view names them.
    assert_eq!(open["subjects"][0]["actor"]["did"], bob.did.as_str());

    // A rule the group's, in its members space, cited.
    let rule = format!("{members}/{did}/{NS}.rule/3mzjdd6rnfise");
    let body =
        json!({"group": did, "subject": alice.did, "reasonType": reason("Rude"), "rule": rule});
    let (status, said) = call(&h, &bob, "mod.submitReport", body).await;
    assert_eq!(status, 200, "{said}");
    let (_, seen) = history(&h, &alice, &did, &alice.did, "").await;
    let event = &seen["events"][0];
    assert_eq!(event["eventId"], said["eventId"]);
    assert_eq!(event["subject"], alice.did.as_str());
    assert_eq!(event["createdBy"], bob.did.as_str());
    assert_eq!(event["creator"]["did"], bob.did.as_str());
    assert_eq!(event["event"]["$type"], format!("{DEFS}#reportEvent"));
    assert_eq!(event["event"]["reasonType"], reason("Rude"));
    assert_eq!(event["event"]["rule"], rule);
    // The account in the queue, by its DID, the newest first when asked.
    let (_, newest) = queue(&h, &alice, &did, "&sort=newest").await;
    assert_eq!(newest["subjects"][0]["subject"], alice.did.as_str());
    assert_eq!(newest["subjects"][0]["actor"]["did"], alice.did.as_str());

    let other = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let in_space = |rest: &str| record(&format!("{space}/{}/{rest}", bob.did));
    for (who, subject, refused) in [
        // Carol cannot read the calendar, nor the members space.
        (&carol, subject.clone(), (400, "InvalidSubject")),
        (&carol, account(&bob.did), (400, "InvalidSubject")),
        // Carol is in no group of Riders'.
        (&bob, account(&carol.did), (400, "InvalidSubject")),
        (
            &bob,
            record(&format!("at://{did}/app.bsky.feed.post/x")),
            (400, "InvalidSubject"),
        ),
        (
            &bob,
            record(&format!(
                "at://{other}/space/{CALENDAR}/rides/{}/c.d.e/f",
                bob.did
            )),
            (400, "InvalidSubject"),
        ),
        (
            &bob,
            record(&format!(
                "at://{did}/space/{CALENDAR}/none/{}/c.d.e/f",
                bob.did
            )),
            (400, "InvalidSubject"),
        ),
        // Each part of its URI what it says, so none is unbounded.
        (
            &bob,
            in_space(&format!("c.d.e/{}", "r".repeat(513))),
            (400, "InvalidSubject"),
        ),
        // A URI that is neither an account's DID nor an at:// record's.
        (
            &bob,
            record("https://example.com/x"),
            (400, "InvalidSubject"),
        ),
        (&bob, record("did:nope"), (400, "InvalidSubject")),
        (&bob, record("not a uri"), (400, "InvalidRequest")),
        (&bob, json!({}), (400, "InvalidRequest")),
        (
            &bob,
            json!({"subject": uri, "cid": "not a cid"}),
            (400, "InvalidRequest"),
        ),
    ] {
        assert_eq!(
            error(&report(&h, who, &did, subject.clone()).await),
            refused,
            "{subject}"
        );
    }
    assert_eq!(
        error(&report(&h, &bob, "did:plc:nothing", subject.clone()).await),
        (400, "GroupNotFound")
    );
    let spam = reason("Spam");
    for body in [
        about(json!({"group": did}), &subject),
        // A reason no labeler of the group's takes: plain words, as
        // reasons were before they were tokens.
        about(json!({"group": did, "reasonType": "spam"}), &subject),
        about(
            json!({"group": did, "reasonType": spam, "reason": "x".repeat(20_001)}),
            &subject,
        ),
        // A rule is in the members space, not another space or a repo.
        about(
            json!({"group": did, "reasonType": spam, "rule": format!("{space}/{did}/{NS}.rule/x")}),
            &subject,
        ),
        about(
            json!({"group": did, "reasonType": spam, "rule": format!("{}/{did}/{NS}.rule/x", meta_space(&did))}),
            &subject,
        ),
        about(
            json!({"group": did, "reasonType": spam, "rule": format!("at://{did}/{NS}.rule/x")}),
            &subject,
        ),
    ] {
        let said = call(&h, &bob, "mod.submitReport", body.clone()).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{body} {}", said.1);
    }
    let lxm = format!("{NS}.mod.submitReport");
    let body = about(json!({"group": did, "reasonType": spam}), &subject);
    assert_eq!(common::groups::post(&h, &lxm, &body, &[]).await.0, 401);
}

/// Puts `labeler` as the group's, in its members space, as it would write
/// it.
async fn put_labeler(h: &Running, did: &str, labeler: Json) {
    let write = Write::Update {
        collection: format!("{NS}.labeler"),
        rkey: "self".into(),
        value: labeler,
    };
    h.host
        .space_write(did, &members_space(did), &[write])
        .await
        .unwrap();
}

#[tokio::test]
async fn takes_only_the_reports_its_labeler_does() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, _]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    let note = record(&format!("{space}/{}/com.example.note/n", bob.did));
    let labeler = json!({
        "$type": format!("{NS}.labeler"),
        "policies": {"labelValues": ["!hide"]},
        "reasonTypes": [reason("Spam")],
        "subjectTypes": ["record"],
        "subjectCollections": ["com.example.event"],
    });
    put_labeler(&h, &did, labeler).await;
    assert_eq!(report(&h, &bob, &did, record(uri)).await.0, 200);
    let rude = about(
        json!({"group": did, "reasonType": reason("Rude")}),
        &record(uri),
    );
    assert_eq!(
        error(&call(&h, &bob, "mod.submitReport", rude).await),
        (400, "InvalidRequest")
    );
    for subject in [account(&alice.did), note] {
        assert_eq!(
            error(&report(&h, &bob, &did, subject.clone()).await),
            (400, "InvalidSubject"),
            "{subject}"
        );
    }
}

#[tokio::test]
async fn takes_an_appeal_of_a_label_or_an_ejection_from_its_subject() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, carol]) = group(&h).await;
    let members = members_space(&did);
    let in_space = post["uri"].as_str().unwrap().to_string();
    let public = format!("at://{}/app.bsky.feed.post/3mzjdd6rnfise", bob.did);

    // Nothing to appeal: a report alone, or nothing at all.
    report(&h, &alice, &did, record(&in_space)).await;
    for subject in [&in_space, &public, &bob.did] {
        let said = appeal(&h, &bob, &did, subject).await;
        assert_eq!(error(&said), (400, "NotAppealable"), "{subject}");
    }

    // Labelled, Bob appeals his post in the calendar, his public one,
    // and his account: each an appeal in the queue, by him.
    labelled(&h, &alice, &did, &space, label(&in_space, "spam")).await;
    labelled(&h, &alice, &did, &members, label(&public, "spam")).await;
    labelled(&h, &alice, &did, &members, label(&bob.did, "troll")).await;
    let (_, view) = update(&h, &alice, &did, &public, json!({"status": "resolved"})).await;
    assert_eq!(view["status"], "resolved");
    for subject in [&in_space, &public, &bob.did] {
        let (status, said) = appeal(&h, &bob, &did, subject).await;
        assert_eq!(status, 200, "{subject} {said}");
        assert!(said["createdAt"].is_string());
        let (_, seen) = history(&h, &alice, &did, subject, "").await;
        let event = &seen["events"][0];
        assert_eq!(event["eventId"], said["eventId"]);
        assert_eq!(event["subject"], subject.as_str());
        assert_eq!(event["createdBy"], bob.did.as_str());
        assert_eq!(
            event["event"],
            json!({"$type": format!("{DEFS}#appealEvent"), "note": "it was a joke"})
        );
    }
    // The appeal reopened what was resolved.
    let (_, open) = queue(&h, &alice, &did, "&status=open").await;
    assert!(listed(&open).contains(&public), "{open}");

    // Not theirs to appeal: another's account or record, nor a record in
    // another group's space.
    let other = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let carols = format!("at://{}/app.bsky.feed.post/3mzjdd6rnfise", carol.did);
    let elsewhere = format!("at://{other}/space/{CALENDAR}/rides/{}/c.d.e/f", bob.did);
    for (who, subject) in [
        (&alice, &in_space),
        (&alice, &public),
        (&alice, &bob.did),
        (&bob, &alice.did),
        (&bob, &carols),
        (&bob, &elsewhere),
        (&bob, &"https://example.com/x".to_string()),
    ] {
        let said = appeal(&h, who, &did, subject).await;
        assert_eq!(error(&said), (400, "InvalidSubject"), "{subject}");
    }

    // A label taken back leaves nothing to appeal.
    let ride = format!("{space}/{}/com.example.event/other", bob.did);
    let (_, made) = labelled(&h, &alice, &did, &space, label(&ride, "spam")).await;
    let rkey = made["uri"].as_str().unwrap().rsplit('/').next().unwrap();
    let nsid = "com.atproto.space.deleteRecord";
    as_group(&h, &alice, &did, &space, nsid, json!({"rkey": rkey})).await;
    let said = appeal(&h, &bob, &did, &ride).await;
    assert_eq!(error(&said), (400, "NotAppealable"));

    // Put out, Carol, who was never labelled, appeals her ejection.
    membership(&h, &did, &carol.did, &["member"]).await;
    eject(&h, &alice, &did, &carol.did).await;
    let (status, said) = appeal(&h, &carol, &did, &carol.did).await;
    assert_eq!(status, 200, "{said}");
    let (_, seen) = history(&h, &alice, &did, &carol.did, "").await;
    assert_eq!(kinds(&seen), ["appealEvent", "ejectEvent"]);
    // Bob, put out too, still appeals his own.
    eject(&h, &alice, &did, &bob.did).await;
    let (status, said) = appeal(&h, &bob, &did, &in_space).await;
    assert_eq!(status, 200, "{said}");

    // A group not here, and a caller who does not say who they are.
    let said = appeal(&h, &bob, "did:plc:nothing", &bob.did).await;
    assert_eq!(error(&said), (400, "GroupNotFound"));
    let lxm = format!("{NS}.mod.submitAppeal");
    let body = json!({"group": did, "subject": bob.did});
    assert_eq!(common::groups::post(&h, &lxm, &body, &[]).await.0, 401);
    let body = json!({"group": did});
    let said = call(&h, &bob, "mod.submitAppeal", body).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

#[tokio::test]
async fn shows_its_subject_what_the_group_did_and_nothing_of_who() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, carol]) = group(&h).await;
    let members = members_space(&did);
    let ride = post["uri"].as_str().unwrap().to_string();
    // Reported, labelled, tagged and resolved with a note; his post
    // labelled; and Alice's account, which is not his.
    report(&h, &alice, &did, account(&bob.did)).await;
    labelled(&h, &alice, &did, &members, label(&bob.did, "troll")).await;
    let change = json!({"addTags": ["urgent"], "status": "resolved", "note": "warned"});
    update(&h, &alice, &did, &bob.did, change).await;
    labelled(&h, &alice, &did, &space, label(&ride, "spam")).await;
    labelled(&h, &alice, &did, &members, label(&alice.did, "x")).await;
    appeal(&h, &bob, &did, &ride).await;

    let (status, mine) = own_history(&h, &bob, &did, "").await;
    assert_eq!(status, 200, "{mine}");
    // The newest first; no report, nor tag.
    assert_eq!(
        kinds(&mine),
        ["appealEvent", "labelEvent", "statusEvent", "labelEvent"]
    );
    let events = mine["events"].as_array().unwrap();
    let subjects: Vec<&str> = events
        .iter()
        .map(|e| e["subject"].as_str().unwrap())
        .collect();
    assert_eq!(subjects, [&ride, &ride, &bob.did, &bob.did]);
    // Neither who acted, nor any note.
    for event in events {
        assert!(event.get("createdBy").is_none(), "{event}");
        assert!(event.get("creator").is_none(), "{event}");
        assert!(event["event"].get("note").is_none(), "{event}");
        assert!(event["eventId"].is_string());
    }
    assert_eq!(
        events[2]["event"],
        json!({"$type": format!("{DEFS}#statusEvent"), "status": "resolved"})
    );
    assert_eq!(events[1]["event"]["val"], "spam");
    assert!(mine.get("cursor").is_none());

    // The oldest first, when asked; a page at a time, either way.
    let (_, oldest) = own_history(&h, &bob, &did, "&sort=oldest").await;
    let mut reversed = kinds(&mine);
    reversed.reverse();
    assert_eq!(kinds(&oldest), reversed);
    for sort in ["newest", "oldest"] {
        let mut seen = Vec::new();
        let mut cursor = String::new();
        loop {
            let more = format!("&sort={sort}&limit=1{cursor}");
            let (_, page) = own_history(&h, &bob, &did, &more).await;
            seen.extend(kinds(&page));
            match page["cursor"].as_str() {
                Some(c) => cursor = format!("&cursor={c}"),
                None => break,
            }
        }
        let all = own_history(&h, &bob, &did, &format!("&sort={sort}")).await;
        assert_eq!(seen, kinds(&all.1), "{sort}");
    }

    // One of his subjects, when asked; another's, never.
    let (_, one) = own_history(&h, &bob, &did, &format!("&subject={}", urlencode(&ride))).await;
    assert_eq!(kinds(&one), ["appealEvent", "labelEvent"]);
    let carols = format!("at://{}/app.bsky.feed.post/3mzjdd6rnfise", carol.did);
    for subject in [alice.did.as_str(), carols.as_str()] {
        let more = format!("&subject={}", urlencode(subject));
        let said = own_history(&h, &bob, &did, &more).await;
        assert_eq!(error(&said), (400, "InvalidSubject"), "{subject}");
    }
    // Nothing for one the group did nothing to.
    let (status, none) = own_history(&h, &carol, &did, "").await;
    assert_eq!((status, none["events"].clone()), (200, json!([])));

    // Put out, Bob still reads it, his ejection with it.
    eject(&h, &alice, &did, &bob.did).await;
    let (status, mine) = own_history(&h, &bob, &did, "").await;
    assert_eq!(status, 200, "{mine}");
    assert_eq!(kinds(&mine)[0], "ejectEvent");

    for (more, refused) in [
        ("&cursor=soon", (400, "InvalidCursor")),
        ("&sort=sideways", (400, "InvalidRequest")),
    ] {
        let said = own_history(&h, &bob, &did, more).await;
        assert_eq!(error(&said), refused, "{more}");
    }
    let said = own_history(&h, &bob, "did:plc:nothing", "").await;
    assert_eq!(error(&said), (400, "GroupNotFound"));
    let lxm = format!("{NS}.mod.querySelfHistory");
    assert_eq!(get(&h, &lxm, &format!("group={did}"), &[]).await.0, 401);
    let said = query(&h, &bob, "mod.querySelfHistory", "").await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

/// A label on `subject` of `val`, as the group writes it.
fn label(subject: &str, val: &str) -> Json {
    json!({"$type": LABEL, "uri": subject, "val": val, "cts": "2026-11-06T00:00:00Z"})
}

/// Asks `nsid`, a write of `com.atproto.space.*`, as the group in `space`
/// with a group token `who` got for it, `body` with the group's repo, the
/// space and the label collection added.
async fn as_group(
    h: &Running,
    who: &Authority,
    group: &str,
    space: &str,
    nsid: &str,
    mut body: Json,
) -> (u16, Json) {
    let app = App::new("console");
    let token = app.token_for(h, who, group, space).await;
    body["repo"] = json!(group);
    body["space"] = json!(space);
    body["collection"] = json!(LABEL);
    let said = app.call(h, nsid, &token, Some(&body)).await;
    (said.status, said.body)
}

/// Writes `record`, a label, as the group in `space`, as `who`.
async fn labelled(
    h: &Running,
    who: &Authority,
    group: &str,
    space: &str,
    record: Json,
) -> (u16, Json) {
    let nsid = "com.atproto.space.createRecord";
    as_group(h, who, group, space, nsid, json!({ "record": record })).await
}

#[tokio::test]
async fn writes_a_label_beside_what_it_labels() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, carol]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    let mut spam = label(uri, "spam");
    spam["cid"] = post["cid"].clone();
    let (status, made) = labelled(&h, &alice, &did, &space, spam.clone()).await;
    assert_eq!(status, 200, "{made}");
    assert_eq!(made["validationStatus"], "valid");
    let at = made["uri"].as_str().unwrap();
    assert!(at.starts_with(&format!("{space}/{did}/{LABEL}/")), "{at}");
    let rkey = at.rsplit('/').next().unwrap().to_string();
    let kept = h.host.group_record(&did, &space, LABEL, &rkey).unwrap();
    assert_eq!(kept, Some(spam));
    // A label opens a subject, though nobody reported it.
    let (_, open) = queue(&h, &alice, &did, "").await;
    assert_eq!(open["subjects"][0]["subject"], uri);
    assert_eq!(open["subjects"][0]["reportCount"], 0);

    // Taken back by deleting it.
    let nsid = "com.atproto.space.deleteRecord";
    let (status, said) = as_group(&h, &alice, &did, &space, nsid, json!({"rkey": rkey})).await;
    assert_eq!(status, 200, "{said}");
    assert_eq!(
        h.host.group_record(&did, &space, LABEL, &rkey).unwrap(),
        None
    );
    // A label nobody wrote is no label taken back.
    let none = json!({"rkey": "3zzzzzzzzzzzz"});
    assert_eq!(as_group(&h, &alice, &did, &space, nsid, none).await.0, 200);
    // Each a label event, the deletion the label taken back.
    let q = format!("group={did}&subject={uri}");
    let (_, seen) = query(
        &h,
        &alice,
        "mod.querySubjectHistory",
        &format!("{q}&sort=oldest"),
    )
    .await;
    assert_eq!(kinds(&seen), ["labelEvent", "labelEvent"]);
    let events = &seen["events"];
    assert_eq!(events[0]["event"]["uri"], made["uri"]);
    assert_eq!(events[0]["event"]["val"], "spam");
    assert_eq!(events[0]["event"]["deleted"], false);
    assert_eq!(events[1]["event"]["uri"], made["uri"]);
    assert_eq!(events[1]["event"]["deleted"], true);
    assert_eq!(events[1]["createdBy"], alice.did.as_str());
    // About the version it named.
    let q = format!("group={did}&subject={uri}&cid={CID}");
    let (_, seen) = query(
        &h,
        &alice,
        "mod.querySubjectHistory",
        &format!("{q}&sort=oldest"),
    )
    .await;
    assert_eq!(kinds(&seen), ["labelEvent", "labelEvent"]);

    // An account's label goes in the members space, whoever they are.
    let members = members_space(&did);
    let (status, made) = labelled(&h, &alice, &did, &members, label(&carol.did, "troll")).await;
    assert_eq!(status, 200, "{made}");
    let q = format!("group={did}&subject={}", carol.did);
    let (_, seen) = query(
        &h,
        &alice,
        "mod.querySubjectHistory",
        &format!("{q}&sort=oldest"),
    )
    .await;
    assert_eq!(seen["events"][0]["event"]["val"], "troll");
    assert_eq!(seen["events"][0]["event"]["uri"], made["uri"]);
    assert_eq!(seen["events"][0]["createdBy"], alice.did.as_str());

    // Beside its subject, and a label as its lexicon has it.
    let elsewhere = format!("{members}/{}/com.example.note/n", bob.did);
    let no_cts = json!({"$type": LABEL, "uri": uri, "val": "x"});
    for (at, record) in [
        (&space, label("not a did", "x")),
        (&space, label(&elsewhere, "x")),
        (&members, label(uri, "x")),
        (&space, label(uri, &"x".repeat(129))),
        (&space, no_cts),
    ] {
        let said = labelled(&h, &alice, &did, at, record.clone()).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{record}");
    }
}

#[tokio::test]
async fn holds_each_label_to_the_action_it_needs() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, _]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    // A member who reads the space writes nothing in it as the group.
    let app = App::new("console");
    let said = group_token(&h, &app, &bob, &did, &space).await;
    assert_eq!(said.status, 403, "{}", said.body);
    use common::members::{granting, keep};
    let labels = group_host::rbac::scope_of("label").unwrap();
    assert_eq!(labels, format!("space:*?collection={LABEL}"));
    let labeler = granting(&h, &did, "labeler", &[&labels], &[]);
    keep(&h, &did, "role", "labeler", labeler).await;
    membership(&h, &did, &bob.did, &["member", "labeler"]).await;
    // Labels alone, of any value, in any of the group's spaces.
    let token = app.token_for(&h, &bob, &did, &space).await;
    let write = |val: &str| json!({"repo": did, "space": space, "collection": LABEL, "record": label(uri, val)});
    let nsid = "com.atproto.space.createRecord";
    for val in ["spam", "!hide", "!takedown"] {
        let said = app.call(&h, nsid, &token, Some(&write(val))).await;
        assert_eq!(said.status, 200, "{val}: {}", said.body);
    }
    let mut other = write("x");
    other["collection"] = json!("com.example.event");
    let said = app.call(&h, nsid, &token, Some(&other)).await;
    assert_eq!(said.status, 403, "{}", said.body);
    let members = members_space(&did);
    let (status, said) = labelled(&h, &bob, &did, &members, label(&alice.did, "x")).await;
    assert_eq!(status, 200, "{said}");
    // No longer theirs to write, once the role is gone.
    membership(&h, &did, &bob.did, &["member"]).await;
    let said = app.call(&h, nsid, &token, Some(&write("spam"))).await;
    assert_eq!(said.status, 403, "{}", said.body);
    assert_eq!(said.body["error"], "Forbidden");
}

/// The spaces `getGroup` says `who` may label in, as the group, in order.
async fn label_spaces(h: &Running, who: &Authority, did: &str) -> Vec<String> {
    let (status, view) = query(h, who, "getGroup", &format!("group={did}")).await;
    assert_eq!(status, 200, "{view}");
    let spaces = view["group"]["viewer"]["labelSpaces"].as_array().unwrap();
    let uri = |s: &Json| s["space"].as_str().unwrap().to_string();
    let mut spaces: Vec<String> = spaces.iter().map(uri).collect();
    spaces.sort();
    spaces
}

#[tokio::test]
async fn labels_only_in_the_spaces_the_roles_scopes_reach() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, _]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    let (meta, members) = (meta_space(&did), members_space(&did));
    use common::members::{granting, keep};
    let moderate = format!("include:{NS}.authModerate");
    let moderator = granting(&h, &did, "moderator", &[&moderate], &[]);
    keep(&h, &did, "role", "moderator", moderator).await;
    membership(&h, &did, &bob.did, &["member", "moderator"]).await;
    // The moderation set labels in the meta and members spaces, as its
    // permissions say.
    let profile = format!("{meta}/{did}/{NS}.about/self");
    let (status, said) = labelled(&h, &bob, &did, &meta, label(&profile, "x")).await;
    assert_eq!(status, 200, "{said}");
    let (status, said) = labelled(&h, &bob, &did, &members, label(&alice.did, "x")).await;
    assert_eq!(status, 200, "{said}");
    assert_eq!(
        label_spaces(&h, &bob, &did).await,
        [members.clone(), meta.clone()]
    );
    // Not in the calendar: no token for it, nothing else being theirs
    // there.
    let app = App::new("console");
    let said = group_token(&h, &app, &bob, &did, &space).await;
    assert_eq!(said.status, 403, "{}", said.body);

    // With labels in every space, there too, of any value.
    let every = format!("space:*?collection={LABEL}");
    let moderator = granting(&h, &did, "moderator", &[&moderate, &every], &[]);
    keep(&h, &did, "role", "moderator", moderator).await;
    let token = app.token_for(&h, &bob, &did, &space).await;
    let write = |val: &str| json!({"repo": did, "space": space, "collection": LABEL, "record": label(uri, val)});
    let nsid = "com.atproto.space.createRecord";
    for val in ["spam", "!takedown"] {
        let said = app.call(&h, nsid, &token, Some(&write(val))).await;
        assert_eq!(said.status, 200, "{val}: {}", said.body);
    }
    let spaces = label_spaces(&h, &bob, &did).await;
    assert_eq!(spaces.len(), 3);
    assert!(spaces.contains(&space), "{spaces:?}");
    // Taken away, the token writes there no more.
    let moderator = granting(&h, &did, "moderator", &[&moderate], &[]);
    keep(&h, &did, "role", "moderator", moderator).await;
    let said = app.call(&h, nsid, &token, Some(&write("spam"))).await;
    assert_eq!(said.status, 403, "{}", said.body);
    assert_eq!(said.body["error"], "Forbidden");

    // A space whose policy grants moderators labels: there, and only
    // there.
    let policy = json!({"$type": "org.atgrp.policy", "grants": [
        {"role": "member", "read": true},
        {"role": "moderator", "read": true, "permissions": [every]},
    ]});
    let board = "com.example.board";
    let board = common::oauth::make_space(&h, &alice, &did, board, "swaps", policy).await;
    let pinned = format!("{board}/{}/com.example.post/p", alice.did);
    let (status, said) = labelled(&h, &bob, &did, &board, label(&pinned, "x")).await;
    assert_eq!(status, 200, "{said}");
    let said = group_token(&h, &app, &bob, &did, &space).await;
    assert_eq!(said.status, 403, "{}", said.body);
    let spaces = label_spaces(&h, &bob, &did).await;
    assert_eq!(spaces.len(), 3);
    assert!(spaces.contains(&board), "{spaces:?}");
    // Those who run the group label in every space.
    assert_eq!(label_spaces(&h, &alice, &did).await.len(), 4);
}

/// Sets the group's labeler to take reports of `types` of subject, of
/// records in `collections`.
async fn takes(h: &Running, did: &str, types: &[&str], collections: &[&str]) {
    let labeler = json!({
        "$type": format!("{NS}.labeler"),
        "policies": {"labelValues": ["!hide"]},
        "subjectTypes": types,
        "subjectCollections": collections,
    });
    put_labeler(h, did, labeler).await;
}

#[tokio::test]
async fn moderates_a_members_public_post_as_its_own_subject() {
    let h = start(&[], false).await;
    let (did, space, _, [alice, bob, carol]) = group(&h).await;
    // Bob's post in his own public repo, in no space of the group's.
    let uri = format!("at://{}/app.bsky.feed.post/3mzjdd6rnfise", bob.did);
    let subject = version(&uri, CID);
    let (status, said) = report(&h, &alice, &did, subject.clone()).await;
    assert_eq!(status, 200, "{said}");
    let (_, open) = queue(&h, &alice, &did, "").await;
    assert_eq!(open["subjects"].as_array().unwrap().len(), 1);
    assert_eq!(open["subjects"][0]["subject"], uri);
    assert_eq!(open["subjects"][0]["reportCount"], 1);
    assert_eq!(open["subjects"][0]["actor"]["did"], bob.did.as_str());
    // Its own subject, not its author's.
    let q = format!("group={did}&subject={}", bob.did);
    let (_, theirs) = query(
        &h,
        &alice,
        "mod.querySubjectHistory",
        &format!("{q}&sort=oldest"),
    )
    .await;
    assert_eq!(theirs["events"], json!([]));
    let q = format!("group={did}&subject={uri}&cid={CID}");
    let (_, seen) = query(
        &h,
        &alice,
        "mod.querySubjectHistory",
        &format!("{q}&sort=oldest"),
    )
    .await;
    assert_eq!(kinds(&seen), ["reportEvent"]);
    assert_eq!(seen["events"][0]["cid"], CID);
    let resolve = json!({"group": did, "subject": uri, "status": "resolved"});
    let (status, view) = call(&h, &alice, "mod.updateSubject", resolve).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["status"], "resolved");
    assert_eq!(view["actor"]["did"], bob.did.as_str());

    let carols = format!("at://{}/app.bsky.feed.post/3mzjdd6rnfise", carol.did);
    for (who, subject) in [
        // Carol cannot read the members space.
        (&carol, record(&uri)),
        // Carol is not a member, nor is the group.
        (&bob, record(&carols)),
        (&bob, record(&format!("at://{did}/app.bsky.feed.post/x"))),
        // A public record, by its DID, collection and key.
        (
            &bob,
            record(&format!("at://{}/app.bsky.feed.post", bob.did)),
        ),
        (&bob, record("at://bob.test/app.bsky.feed.post/x")),
    ] {
        assert_eq!(
            error(&report(&h, who, &did, subject.clone()).await),
            (400, "InvalidSubject"),
            "{subject}"
        );
    }

    // The labeler's kinds and collections, as for any record.
    takes(&h, &did, &["record"], &["app.bsky.feed.post"]).await;
    assert_eq!(report(&h, &bob, &did, record(&uri)).await.0, 200);
    for (types, collections) in [
        (["account"], ["app.bsky.feed.post"]),
        (["record"], ["com.example.event"]),
    ] {
        takes(&h, &did, &types, &collections).await;
        assert_eq!(
            error(&report(&h, &bob, &did, record(&uri)).await),
            (400, "InvalidSubject"),
            "{types:?} {collections:?}"
        );
    }

    // Labelled in the members space, where its author's labels go.
    let members = members_space(&did);
    let (status, made) = labelled(&h, &alice, &did, &members, label(&uri, "spam")).await;
    assert_eq!(status, 200, "{made}");
    let rkey = made["uri"].as_str().unwrap().rsplit('/').next().unwrap();
    let nsid = "com.atproto.space.deleteRecord";
    let (status, said) = as_group(&h, &alice, &did, &members, nsid, json!({"rkey": rkey})).await;
    assert_eq!(status, 200, "{said}");
    let q = format!("group={did}&subject={uri}");
    let (_, seen) = query(
        &h,
        &alice,
        "mod.querySubjectHistory",
        &format!("{q}&sort=oldest"),
    )
    .await;
    assert_eq!(
        kinds(&seen),
        [
            "reportEvent",
            "statusEvent",
            "reportEvent",
            "labelEvent",
            "labelEvent"
        ]
    );
    let events = &seen["events"];
    assert_eq!(events[3]["event"]["uri"], made["uri"]);
    assert_eq!(events[3]["event"]["val"], "spam");
    assert_eq!(events[3]["event"]["deleted"], false);
    assert_eq!(events[3]["createdBy"], alice.did.as_str());
    assert_eq!(events[4]["event"]["uri"], made["uri"]);
    assert_eq!(events[4]["event"]["deleted"], true);
    assert_eq!(events[4]["createdBy"], alice.did.as_str());
    // A non-member's public record, or one labelled in any other space.
    for (at, subject) in [
        (&members, &carols),
        (&space, &uri),
        (&meta_space(&did), &uri),
    ] {
        let said = labelled(&h, &alice, &did, at, label(subject, "x")).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{at} {subject}");
    }
}

#[tokio::test]
async fn updates_a_subjects_status_tags_and_notes() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, _]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    report(&h, &bob, &did, record(uri)).await;
    let (status, view) = update(&h, &alice, &did, uri, json!({"status": "resolved"})).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["status"], "resolved");
    assert_eq!(view["subject"], uri);
    assert_eq!(view["reportCount"], 1);
    assert_eq!(view["tags"], json!([]));
    assert!(view["actor"]["did"].is_string());
    // A note alone keeps the status it has.
    let (_, view) = update(&h, &alice, &did, uri, json!({"note": "Asked them"})).await;
    assert_eq!(view["status"], "resolved");
    // Reopened; then tagged, the tags in order whatever order they came.
    let (_, view) = update(&h, &alice, &did, uri, json!({"status": "open"})).await;
    assert_eq!(view["status"], "open");
    let tag = json!({"addTags": ["urgent", "spam"]});
    let (_, view) = update(&h, &alice, &did, uri, tag).await;
    assert_eq!(view["tags"], json!(["spam", "urgent"]));
    assert_eq!(view["status"], "open");
    // A status and tags at once, each its own event; nothing asked,
    // nothing changes.
    let both = json!({"status": "resolved", "removeTags": ["spam"]});
    let (_, view) = update(&h, &alice, &did, uri, both).await;
    assert_eq!(view["tags"], json!(["urgent"]));
    assert_eq!(view["status"], "resolved");
    let (status, same) = update(&h, &alice, &did, uri, json!({})).await;
    assert_eq!((status, &same), (200, &view));

    // A label leaves it resolved; a new report opens it again.
    let mut spam = label(uri, "spam");
    spam["cid"] = json!(CID);
    labelled(&h, &alice, &did, &space, spam).await;
    let count = async |status: &str| {
        let (_, q) = queue(&h, &alice, &did, &format!("&status={status}")).await;
        listed(&q).len()
    };
    assert_eq!(count("resolved").await, 1);
    report(&h, &bob, &did, version(uri, CID)).await;
    assert_eq!(count("open").await, 1);

    // The newest first unless asked.
    let (_, all) = history(&h, &alice, &did, uri, "&sort=oldest").await;
    assert_eq!(
        kinds(&all),
        [
            "reportEvent",
            "statusEvent",
            "statusEvent",
            "statusEvent",
            "tagEvent",
            "statusEvent",
            "tagEvent",
            "labelEvent",
            "reportEvent"
        ]
    );
    let (_, newest) = history(&h, &alice, &did, uri, "").await;
    let mut reversed = kinds(&all);
    reversed.reverse();
    assert_eq!(kinds(&newest), reversed);
    let events = &all["events"];
    assert_eq!(events[0]["event"]["reasonType"], reason("Spam"));
    assert_eq!(events[0]["event"]["reason"], "ads");
    assert_eq!(events[0]["subject"], uri);
    assert!(events[1]["event"].get("note").is_none());
    assert_eq!(
        events[2]["event"],
        json!({"$type": format!("{DEFS}#statusEvent"), "status": "resolved", "note": "Asked them"})
    );
    assert_eq!(events[3]["event"]["status"], "open");
    assert_eq!(
        events[4]["event"],
        json!({"$type": format!("{DEFS}#tagEvent"), "add": ["urgent", "spam"]})
    );
    assert_eq!(
        events[6]["event"],
        json!({"$type": format!("{DEFS}#tagEvent"), "remove": ["spam"]})
    );
    assert_eq!(events[4]["createdBy"], alice.did.as_str());
    assert!(all.get("cursor").is_none());
    // Narrowed to one version of the record: what was said of it.
    let (_, version) = history(&h, &alice, &did, uri, &format!("&cid={CID}&sort=oldest")).await;
    assert_eq!(kinds(&version), ["labelEvent", "reportEvent"]);
    // Each names the version it is about, for a moderator to act on it.
    assert_eq!(version["events"][0]["cid"], CID);
    assert!(events[2].get("cid").is_none());
    // A page at a time, in either order.
    for sort in ["newest", "oldest"] {
        let mut seen = Vec::new();
        let mut cursor = String::new();
        loop {
            let more = format!("&sort={sort}&limit=4{cursor}");
            let (_, page) = history(&h, &alice, &did, uri, &more).await;
            seen.extend(kinds(&page));
            match page["cursor"].as_str() {
                Some(c) => cursor = format!("&cursor={c}"),
                None => break,
            }
        }
        let (_, whole) = history(&h, &alice, &did, uri, &format!("&sort={sort}")).await;
        assert_eq!(seen, kinds(&whole), "{sort}");
    }
    for (more, refused) in [
        ("&cursor=soon", "InvalidCursor"),
        ("&sort=sideways", "InvalidRequest"),
    ] {
        let said = history(&h, &alice, &did, uri, more).await;
        assert_eq!(error(&said), (400, refused), "{more}");
    }
    // An account nobody has said anything of has no history.
    let (status, none) = history(&h, &alice, &did, &bob.did, "").await;
    assert_eq!((status, none["events"].clone()), (200, json!([])));
    let said = history(&h, &alice, &did, "at://nothing", "").await;
    assert_eq!(error(&said), (400, "InvalidSubject"));

    let long = "x".repeat(20_001);
    let never = format!("{uri}2");
    let elsewhere = format!(
        "at://did:plc:other/space/{CALENDAR}/rides/{}/c.d.e/f",
        bob.did
    );
    let many: Vec<String> = (0..26).map(|i| "a".repeat(i + 1)).collect();
    for (who, subject, change, refused) in [
        (
            &alice,
            uri,
            json!({"status": "closed"}),
            (400, "InvalidRequest"),
        ),
        (&alice, uri, json!({"note": long}), (400, "InvalidRequest")),
        (
            &alice,
            uri,
            json!({"addTags": ["Urgent"]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            uri,
            json!({"addTags": ["two words"]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            uri,
            json!({"removeTags": ["x_y"]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            uri,
            json!({"addTags": many}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            never.as_str(),
            json!({"status": "resolved"}),
            (400, "SubjectNotFound"),
        ),
        (
            &alice,
            elsewhere.as_str(),
            json!({"status": "resolved"}),
            (400, "InvalidSubject"),
        ),
        (
            &alice,
            "not a uri",
            json!({"status": "resolved"}),
            (400, "InvalidRequest"),
        ),
        (&bob, uri, json!({"status": "resolved"}), (403, "Forbidden")),
    ] {
        let said = update(&h, who, &did, subject, change.clone()).await;
        assert_eq!(error(&said), refused, "{subject} {change}");
    }
    let body = json!({"group": did, "status": "resolved"});
    let said = call(&h, &alice, "mod.updateSubject", body).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

#[tokio::test]
async fn lists_the_queue_by_status_tags_and_order_a_page_at_a_time() {
    let h = start(&[], false).await;
    let (did, space, _, [alice, bob, _]) = group(&h).await;
    let subjects: Vec<String> = (0..3)
        .map(|i| format!("{space}/{}/com.example.event/r{i}", bob.did))
        .collect();
    let (r0, r1, r2) = (&*subjects[0], &*subjects[1], &*subjects[2]);
    for s in &subjects {
        report(&h, &bob, &did, record(s)).await;
        tick().await;
    }
    // Changed in an order of their own: r1 tagged, r0 resolved, r2 tagged.
    update(&h, &alice, &did, r1, json!({"addTags": ["urgent"]})).await;
    tick().await;
    update(&h, &alice, &did, r0, json!({"status": "resolved"})).await;
    tick().await;
    update(&h, &alice, &did, r2, json!({"addTags": ["spam"]})).await;
    let listing = async |more: &str| {
        let (status, q) = queue(&h, &alice, &did, more).await;
        assert_eq!(status, 200, "{q}");
        listed(&q)
    };
    // Every status unless asked, the oldest first.
    assert_eq!(listing("").await, [r0, r1, r2]);
    assert_eq!(listing("&sort=oldest").await, [r0, r1, r2]);
    assert_eq!(listing("&sort=newest").await, [r2, r1, r0]);
    assert_eq!(listing("&sort=updated").await, [r2, r0, r1]);
    assert_eq!(listing("&status=open").await, [r1, r2]);
    assert_eq!(listing("&status=resolved").await, [r0]);
    // Any of the tags asked.
    assert_eq!(listing("&tags=urgent").await, [r1]);
    assert_eq!(listing("&tags=urgent&tags=spam").await, [r1, r2]);
    assert_eq!(
        listing("&tags=spam&status=resolved").await,
        Vec::<String>::new()
    );
    assert_eq!(listing("&tags=none").await, Vec::<String>::new());
    let (_, q) = queue(&h, &alice, &did, "&tags=urgent").await;
    assert_eq!(q["subjects"][0]["tags"], json!(["urgent"]));

    // A page at a time, in each order.
    for sort in ["oldest", "newest", "updated"] {
        let mut seen = Vec::new();
        let mut cursor = String::new();
        loop {
            let (_, page) = queue(&h, &alice, &did, &format!("&sort={sort}&limit=1{cursor}")).await;
            seen.extend(listed(&page));
            match page["cursor"].as_str() {
                Some(c) => cursor = format!("&cursor={}", urlencode(c)),
                None => break,
            }
        }
        assert_eq!(seen, listing(&format!("&sort={sort}")).await, "{sort}");
    }

    for (more, refused) in [
        ("&cursor=soon", "InvalidCursor"),
        ("&status=closed", "InvalidRequest"),
        ("&sort=sideways", "InvalidRequest"),
    ] {
        let said = queue(&h, &alice, &did, more).await;
        assert_eq!(error(&said), (400, refused), "{more}");
    }
    assert_eq!(error(&queue(&h, &bob, &did, "").await), (403, "Forbidden"));
    // A member's service auth names the group.
    let said = query(&h, &alice, "mod.querySubjects", "").await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

#[tokio::test]
async fn moderates_as_the_group_a_session_is_for() {
    let h = start(&[], false).await;
    let (did, _, post, [alice, bob, _]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    report(&h, &bob, &did, record(uri)).await;
    let methods = ["querySubjects", "querySubjectHistory", "updateSubject"];
    let rpc: Vec<String> = methods
        .iter()
        .map(|m| format!("rpc:{NS}.mod.{m}?aud=*"))
        .collect();
    let app = App::loopback(&format!("atproto {}", rpc.join(" ")));
    let tokens = app.sign_in(&h, &alice, &did).await;
    let access = tokens["access_token"].as_str().unwrap();
    let get = async |q: String| app.call(&h, &format!("{NS}.mod.{q}"), access, None).await;
    // No group named: the session's.
    let said = get("querySubjects".into()).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(listed(&said.body), [uri]);
    let said = get(format!("querySubjects?group={did}")).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let said = get(format!("querySubjectHistory?subject={}", urlencode(uri))).await;
    assert_eq!(kinds(&said.body), ["reportEvent"]);
    let change = json!({"subject": uri, "addTags": ["urgent"]});
    let nsid = format!("{NS}.mod.updateSubject");
    let said = app.call(&h, &nsid, access, Some(&change)).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["tags"], json!(["urgent"]));
    // Another group named is another session's.
    let said = get("querySubjects?group=did:plc:elsewhere".into()).await;
    assert_eq!(said.error(), (403, "Forbidden"));
    // Without a session, a member's service auth names the group.
    let said = call(&h, &alice, "mod.updateSubject", change).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    let q = format!("subject={}", urlencode(uri));
    let said = query(&h, &alice, "mod.querySubjectHistory", &q).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
}

fn urlencode(text: &str) -> String {
    reqwest::Url::parse_with_params("http://x/", &[("q", text)])
        .unwrap()
        .query()
        .unwrap()[2..]
        .to_string()
}

#[test]
fn finds_the_space_a_record_is_in() {
    assert_eq!(
        space_of("at://did:plc:a/space/x.y.z/k/did:plc:b/c.d.e/r"),
        Some(("at://did:plc:a/space/x.y.z/k".into(), "did:plc:a".into()))
    );
    assert_eq!(space_of("at://did:plc:a/c.d.e/r"), None);
    assert_eq!(space_of("https://x"), None);
}

#[tokio::test]
async fn answers_an_ejection_as_one() {
    let h = start(&[], false).await;
    let (did, _, _, [alice, _, carol]) = group(&h).await;
    let event = Event {
        kind: "eject".into(),
        actor: Some(alice.did.clone()),
        created_at: 1,
        ..Event::default()
    };
    h.host.store.moderate(&did, &carol.did, &event).unwrap();
    let (_, seen) = history(&h, &alice, &did, &carol.did, "").await;
    assert_eq!(seen["events"][0]["subject"], carol.did.as_str());
    assert_eq!(
        seen["events"][0]["event"],
        json!({"$type": format!("{DEFS}#ejectEvent")})
    );
}

#[tokio::test]
async fn defines_the_groups_own_labels_for_one_who_configures_it() {
    let h = start(&[], false).await;
    let (did, _, _, [alice, bob, _]) = group(&h).await;
    let labels = async || {
        let (_, view) = query(&h, &alice, "getGroup", &format!("group={did}")).await;
        view["group"]["labels"].clone()
    };
    // A new group defines none of its own.
    assert_eq!(labels().await, json!([]));
    let troll = json!({"group": did, "label": "troll", "name": "Troll", "description": "Picks fights on purpose."});
    let (status, defined) = call(&h, &alice, "putLabelDefinition", troll.clone()).await;
    assert_eq!(status, 200, "{defined}");
    assert_eq!(defined["identifier"], "troll");
    assert_eq!(defined["severity"], "inform");
    assert_eq!(defined["blurs"], "none");
    assert_eq!(defined["defaultSetting"], "warn");
    assert_eq!(
        defined["locales"][0],
        json!({"lang": "en", "name": "Troll", "description": "Picks fights on purpose."})
    );
    assert!(defined.get("adultOnly").is_none());
    assert_eq!(labels().await, json!([defined]));
    // Defined again, it is changed in place, not listed twice; and it is
    // among the values the labeler applies, beside atproto's.
    let again = json!({"group": did, "label": "troll", "name": "Troll", "description": "Baits people.", "lang": "sl", "severity": "alert", "blurs": "content", "defaultSetting": "hide", "adultOnly": true});
    let (_, changed) = call(&h, &alice, "putLabelDefinition", again).await;
    assert_eq!(changed["adultOnly"], true);
    assert_eq!(changed["severity"], "alert");
    // It keeps its English name beside the Slovenian one.
    assert_eq!(changed["locales"][0]["lang"], "en");
    assert_eq!(changed["locales"][1]["lang"], "sl");
    assert_eq!(labels().await, json!([changed]));
    // A French name joins them, and a new English one replaces only the
    // English, in its place.
    let fr = json!({"group": did, "label": "troll", "name": "Troll (fr)", "description": "Cherche la bagarre.", "lang": "fr"});
    let (status, _) = call(&h, &alice, "putLabelDefinition", fr).await;
    assert_eq!(status, 200);
    let en = json!({"group": did, "label": "troll", "name": "Provoker", "description": "Stirs things up."});
    let (status, changed) = call(&h, &alice, "putLabelDefinition", en).await;
    assert_eq!(status, 200);
    let fr = json!({"lang": "fr", "name": "Troll (fr)", "description": "Cherche la bagarre."});
    assert_eq!(
        changed["locales"],
        json!([
            {"lang": "en", "name": "Provoker", "description": "Stirs things up."},
            {"lang": "sl", "name": "Troll", "description": "Baits people."},
            fr,
        ])
    );
    let members = members_space(&did);
    let labeler = h
        .host
        .group_record(&did, &members, "org.atgrp.labeler", "self");
    let labeler = labeler.unwrap().unwrap();
    assert_eq!(
        labeler["policies"]["labelValues"],
        json!(["!hide", "!takedown", "troll"])
    );
    assert_eq!(labeler["subjectTypes"], json!(["account", "record"]));
    assert_eq!(
        labeler["policies"]["labelValueDefinitions"],
        json!([changed])
    );
    // atproto's own values, and anything else, are not the group's to define.
    for label in ["!hide", "Troll", "spam bot", ""] {
        let body = json!({"group": did, "label": label, "name": "x", "description": "x"});
        let (status, said) = call(&h, &alice, "putLabelDefinition", body).await;
        assert_eq!(status, 400, "{label}: {said}");
        let (status, _) = call(
            &h,
            &alice,
            "deleteLabelDefinition",
            json!({"group": did, "label": label}),
        )
        .await;
        assert_eq!(status, 400, "{label}");
    }
    // Bob, a member, does not configure it.
    let (status, said) = call(&h, &bob, "putLabelDefinition", troll).await;
    assert_eq!((status, said["error"].as_str()), (403, Some("Forbidden")));
    let (status, _) = call(
        &h,
        &bob,
        "deleteLabelDefinition",
        json!({"group": did, "label": "troll"}),
    )
    .await;
    assert_eq!(status, 403);
    // Taken away, it is neither defined nor applied, and atproto's stay.
    let (status, said) = call(
        &h,
        &alice,
        "deleteLabelDefinition",
        json!({"group": did, "label": "troll"}),
    )
    .await;
    assert_eq!((status, said), (200, json!({})));
    assert_eq!(labels().await, json!([]));
    let labeler = h
        .host
        .group_record(&did, &members, "org.atgrp.labeler", "self");
    assert_eq!(
        labeler.unwrap().unwrap()["policies"]["labelValues"],
        json!(["!hide", "!takedown"])
    );
}

/// What `who` reads of the events in `space`, Riders', through the
/// host's index: each record's URI, in order; and with `uri`, that one's.
async fn events(
    h: &Running,
    who: &Authority,
    did: &str,
    space: &str,
    uri: Option<&str>,
) -> Vec<String> {
    let lxm = format!("{INTERNAL}.listSpaceRecords");
    let mut q = format!("group={did}&space={space}&collection=com.example.event");
    if let Some(uri) = uri {
        q = format!("group={did}&space={space}&uri={uri}");
    }
    let (status, page) = get(h, &lxm, &q, &[auth(h, who, &lxm)]).await;
    assert_eq!(status, 200, "{page}");
    let records = page["records"].as_array().unwrap().iter();
    let mut uris: Vec<String> = records.map(|r| r["uri"].as_str().unwrap().into()).collect();
    uris.sort();
    uris
}

#[tokio::test]
async fn takes_out_of_the_space_what_the_group_takes_down() {
    let h = start(&[], false).await;
    let (did, space, _, [alice, bob, carol]) = group(&h).await;
    // Dan, an account here and a member, posts a picnic, and a ride with
    // a map.
    let dan = h.host.create_account("dan.groups.test").await.unwrap();
    membership(&h, &did, &dan, &["member"]).await;
    let map = h.host.upload_blob(&dan, b"a map", "image/png").unwrap();
    let event = |rkey: &str, value: Json| Write::Create {
        collection: "com.example.event".into(),
        rkey: rkey.into(),
        value,
    };
    let writes = [
        event(
            "picnic",
            json!({"$type": "com.example.event", "text": "Picnic"}),
        ),
        event(
            "ride",
            json!({"$type": "com.example.event", "text": "Ride", "map": map}),
        ),
    ];
    h.host.space_write(&dan, &space, &writes).await.unwrap();
    let picnic = format!("{space}/{dan}/com.example.event/picnic");
    let ride = format!("{space}/{dan}/com.example.event/ride");
    let both = vec![picnic.clone(), ride.clone()];
    let blob = format!("{INTERNAL}.getSpaceBlob");
    let cid = map["ref"]["$link"].as_str().unwrap();
    let q = format!("group={did}&space={space}&repo={dan}&cid={cid}");
    let the_map = async || read(&h, &blob, &q, &[auth(&h, &bob, &blob)]).await;
    assert_eq!(events(&h, &bob, &did, &space, None).await, both);
    assert_eq!(the_map().await.status, 200);

    // The group takes the ride down: it and its map are no longer served.
    let (status, made) = labelled(&h, &alice, &did, &space, label(&ride, "!takedown")).await;
    assert_eq!(status, 200, "{made}");
    let down = made["uri"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(
        events(&h, &bob, &did, &space, None).await,
        [picnic.as_str()]
    );
    assert!(events(&h, &bob, &did, &space, Some(&ride)).await.is_empty());
    assert_eq!(the_map().await.error(), (400, "NotFound".into()));

    // No other label takes anything down: another value, a takedown of
    // another version, one lapsed, or one Dan writes himself.
    let mut versioned = label(&picnic, "!takedown");
    versioned["cid"] = json!(CID);
    let mut lapsed = label(&picnic, "!takedown");
    lapsed["exp"] = json!("2000-01-01T00:00:00Z");
    for record in [label(&picnic, "!hide"), versioned, lapsed] {
        let (status, made) = labelled(&h, &alice, &did, &space, record).await;
        assert_eq!(status, 200, "{made}");
    }
    let own = Write::Create {
        collection: LABEL.into(),
        rkey: "3mzjdd6rnfise".into(),
        value: label(&picnic, "!takedown"),
    };
    h.host.space_write(&dan, &space, &[own]).await.unwrap();
    assert_eq!(
        events(&h, &bob, &did, &space, None).await,
        [picnic.as_str()]
    );

    // A takedown of the version it is now takes it down.
    let lxm = format!("{INTERNAL}.listSpaceRecords");
    let one = format!("group={did}&space={space}&uri={picnic}");
    let (_, page) = get(&h, &lxm, &one, &[auth(&h, &bob, &lxm)]).await;
    let mut now = label(&picnic, "!takedown");
    now["cid"] = page["records"][0]["cid"].clone();
    let (status, made) = labelled(&h, &alice, &did, &space, now).await;
    assert_eq!(status, 200, "{made}");
    assert!(events(&h, &bob, &did, &space, None).await.is_empty());

    // Deleting the takedowns puts both back, and the map.
    let nsid = "com.atproto.space.deleteRecord";
    let again = made["uri"].as_str().unwrap().rsplit('/').next().unwrap();
    for rkey in [down.as_str(), again] {
        let (status, said) = as_group(&h, &alice, &did, &space, nsid, json!({"rkey": rkey})).await;
        assert_eq!(status, 200, "{said}");
    }
    assert_eq!(events(&h, &bob, &did, &space, None).await, both);
    assert_eq!(the_map().await.status, 200);

    // Dan's account taken down, in the members space, takes all he wrote
    // out of every space; another's takes nothing of his.
    let members = members_space(&did);
    labelled(&h, &alice, &did, &members, label(&carol.did, "!takedown")).await;
    assert_eq!(events(&h, &bob, &did, &space, None).await, both);
    let (status, made) = labelled(&h, &alice, &did, &members, label(&dan, "!takedown")).await;
    assert_eq!(status, 200, "{made}");
    assert!(events(&h, &bob, &did, &space, None).await.is_empty());
    assert_eq!(the_map().await.error(), (400, "NotFound".into()));
}

/// How the console's `/img` answers `who`, signed in to it, for `did`'s
/// picture `cid`: by its status.
async fn picture(h: &Running, who: &Authority, did: &str, cid: &str) -> u16 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let cookie = h.host.session_cookie(&who.did, now);
    let name = h.host.cookie_name(group_host::console_login::COOKIE);
    let req = h.http.get(format!("{}/img/{did}/{cid}", h.url));
    let res = req
        .header("cookie", format!("{name}={cookie}"))
        .send()
        .await;
    res.unwrap().status().as_u16()
}

/// A subject's history, as `who` reviews it in Riders' queue.
async fn reviewed(h: &Running, who: &Authority, did: &str, subject: &str) -> (u16, Json) {
    history(h, who, did, subject, "").await
}

/// The file `cid` of the record `subject`, as `who` reviews it.
async fn reviewed_file(h: &Running, who: &Authority, did: &str, subject: &str, cid: &str) -> Read {
    let lxm = format!("{INTERNAL}.getModSubjectBlob");
    let q = format!("group={did}&subject={}&cid={cid}", urlencode(subject));
    read(h, &lxm, &q, &[auth(h, who, &lxm)]).await
}

#[tokio::test]
async fn shows_a_moderator_what_was_taken_down_where_they_review_it() {
    let h = start(&[], false).await;
    let (did, space, _, [alice, bob, carol]) = group(&h).await;
    // Dan, an account here and a member, posts a ride with a map, which
    // the group takes down; and a picnic with a flyer, which it leaves.
    let dan = h.host.create_account("dan.groups.test").await.unwrap();
    membership(&h, &did, &dan, &["member"]).await;
    let map = h.host.upload_blob(&dan, b"a map", "image/png").unwrap();
    let flyer = h.host.upload_blob(&dan, b"a flyer", "image/png").unwrap();
    let event = |rkey: &str, value: Json| Write::Create {
        collection: "com.example.event".into(),
        rkey: rkey.into(),
        value,
    };
    let ride_value = json!({"$type": "com.example.event", "text": "Ride", "map": map});
    let picnic_value = json!({"$type": "com.example.event", "text": "Picnic", "flyer": flyer});
    let writes = [
        event("ride", ride_value.clone()),
        event("picnic", picnic_value),
    ];
    h.host.space_write(&dan, &space, &writes).await.unwrap();
    let ride = format!("{space}/{dan}/com.example.event/ride");
    let picnic = format!("{space}/{dan}/com.example.event/picnic");
    let (status, made) = labelled(&h, &alice, &did, &space, label(&ride, "!takedown")).await;
    assert_eq!(status, 200, "{made}");
    let map = map["ref"]["$link"].as_str().unwrap();
    let flyer = flyer["ref"]["$link"].as_str().unwrap();

    // Read as anyone reads the space, it is gone for a member and for the
    // moderator alike, and its map with it, as a file and as a picture;
    // the picnic's flyer, which nothing taken down holds, is not.
    for who in [&bob, &alice] {
        assert_eq!(events(&h, who, &did, &space, None).await, [picnic.as_str()]);
        let lxm = format!("{INTERNAL}.getSpaceBlob");
        let q = format!("group={did}&space={space}&repo={dan}&cid={map}");
        let file = read(&h, &lxm, &q, &[auth(&h, who, &lxm)]).await;
        assert_eq!(file.error(), (400, "NotFound".into()));
        assert_eq!(picture(&h, who, &dan, map).await, 404);
        assert_eq!(picture(&h, who, &dan, flyer).await, 200);
    }

    // Reviewed, the moderator sees it as the index holds it, said to be
    // taken down, and its map.
    let (status, seen) = reviewed(&h, &alice, &did, &ride).await;
    assert_eq!(status, 200, "{seen}");
    let record = &seen["record"];
    assert_eq!(record["uri"], ride.as_str());
    assert_eq!(record["value"], ride_value);
    assert_eq!(record["takenDown"], true);
    assert!(record["cid"].is_string());
    assert!(record["createdAt"].is_string());
    assert_eq!(kinds(&seen), ["labelEvent"]);
    let got = reviewed_file(&h, &alice, &did, &ride, map).await;
    assert_eq!(got.status, 200);
    assert_eq!(got.body, b"a map");
    assert_eq!(got.header("content-type"), Some("image/png"));
    assert_eq!(
        got.header("content-security-policy"),
        Some("default-src 'none'; sandbox")
    );

    // What is not taken down is shown as well, said to be up; only the
    // files the record holds are served, not another of the writer's.
    let (_, seen) = reviewed(&h, &alice, &did, &picnic).await;
    assert_eq!(seen["record"]["takenDown"], false);
    assert!(seen["events"].as_array().unwrap().is_empty());
    assert_eq!(
        reviewed_file(&h, &alice, &did, &picnic, flyer).await.status,
        200
    );
    let not_its = reviewed_file(&h, &alice, &did, &ride, flyer).await;
    assert_eq!(not_its.error(), (400, "NotFound".into()));

    // An account, a record the index does not hold, a public record, and
    // one in a space not hosted here carry no record, and serve no file.
    let gone = format!("{space}/{dan}/com.example.event/gone");
    let public = format!("at://{dan}/com.example.event/ride");
    let nowhere = format!("at://{did}/space/{CALENDAR}/nowhere/{dan}/com.example.event/ride");
    for subject in [dan.as_str(), &gone, &public, &nowhere] {
        let (status, seen) = reviewed(&h, &alice, &did, subject).await;
        assert_eq!(status, 200, "{seen}");
        assert!(seen.get("record").is_none(), "{seen}");
        let none = reviewed_file(&h, &alice, &did, subject, map).await;
        assert_eq!(none.error(), (400, "NotFound".into()));
    }
    let bad = reviewed_file(&h, &alice, &did, &ride, "bafynotacid").await;
    assert_eq!(bad.error(), (400, "InvalidRequest".into()));
    let elsewhere = "at://did:plc:x/space/a.b/c/did:plc:y/a.b/c";
    let elsewhere = reviewed_file(&h, &alice, &did, elsewhere, map).await;
    assert_eq!(elsewhere.error(), (400, "InvalidSubject".into()));

    // A plain member is refused both.
    assert_eq!(reviewed(&h, &bob, &did, &ride).await.0, 403);
    let refused = reviewed_file(&h, &bob, &did, &ride, map).await;
    assert_eq!(refused.error(), (403, "Forbidden".into()));

    // A moderator the space does not let read sees the history, and
    // neither the record nor its files.
    let role = json!({"$type": format!("{NS}.role"), "displayName": "Moderator", "permissions": [format!("rpc:{NS}.mod.querySubjects?aud=*")]});
    let write = Write::Create {
        collection: format!("{NS}.role"),
        rkey: "moderator".into(),
        value: role,
    };
    let members = members_space(&did);
    h.host.space_write(&did, &members, &[write]).await.unwrap();
    membership(&h, &did, &carol.did, &["moderator"]).await;
    let (status, seen) = reviewed(&h, &carol, &did, &ride).await;
    assert_eq!(status, 200, "{seen}");
    assert_eq!(kinds(&seen), ["labelEvent"]);
    assert!(seen.get("record").is_none(), "{seen}");
    let unread = reviewed_file(&h, &carol, &did, &ride, map).await;
    assert_eq!(unread.error(), (400, "NotFound".into()));
}
