//! A group's moderation: reports from those who can read what they
//! report, labels written beside what they label, and the queue between.

mod common;

use common::groups::*;
use common::oauth::{App, group_token};
use common::spaces::*;
use common::{Running, start};
use group_host::group::{members_space, meta_space};
use group_host::moderation::{Event, space_of};
use group_host::repo::Write;
use group_host::store::Store;
use serde_json::{Value as Json, json};

const CALENDAR: &str = "com.example.calendar";
const LABEL: &str = "com.atproto.label.record";
const CID: &str = "bafyreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm";

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
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn query(h: &Running, who: &Authority, method: &str, q: &str) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
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
    let body = json!({"group": group, "reasonType": "spam", "reason": "ads"});
    call(h, who, "createReport", about(body, &subject)).await
}

const APPEAL: &str = "appeal";

/// `who` appeals what the group did to `subject`, as a report of it.
async fn appeal(h: &Running, who: &Authority, group: &str, subject: Json) -> (u16, Json) {
    let body = json!({"group": group, "reasonType": APPEAL, "reason": "it was a joke"});
    call(h, who, "createReport", about(body, &subject)).await
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

/// Each event's kind, as the history answers it: `reportEvent`, …
fn kinds(history: &Json) -> Vec<String> {
    let events = history["events"].as_array().unwrap();
    let kind = |e: &Json| {
        e["event"]["$type"]
            .as_str()
            .unwrap()
            .replace("fyi.opensocial.defs#", "")
    };
    events.iter().map(kind).collect()
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
    let readers = json!({"$type": "fyi.opensocial.policy", "grants": [{"role": "member", "read": true, "write": true}]});
    let space = common::oauth::make_space(h, &alice, &did, CALENDAR, "rides", readers).await;
    membership(h, &did, &bob.did, &["member"]).await;
    let post = json!({"$type": "com.atproto.repo.strongRef", "uri": format!("{space}/{}/com.example.event/ride", bob.did), "cid": CID});
    (did, space, post, [alice, bob, carol])
}

#[tokio::test]
async fn takes_a_report_from_one_who_reads_it() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, carol]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    let subject = version(uri, CID);
    let (status, said) = report(&h, &bob, &did, subject.clone()).await;
    assert_eq!(status, 200, "{said}");
    assert!(said["createdAt"].is_string());
    // Each report its own id, however close together they come.
    let (_, again) = report(&h, &bob, &did, subject.clone()).await;
    assert_ne!(said["id"], again["id"]);
    assert!(said["id"].is_string());
    let (_, queue) = query(&h, &alice, "listModSubjects", &format!("group={did}")).await;
    // A subject as its URI, or an account's DID, as it was sent.
    assert_eq!(queue["subjects"][0]["subject"], uri);
    assert_eq!(queue["subjects"][0]["status"], "open");
    assert_eq!(queue["subjects"][0]["reportCount"], 2);
    // Whose record it is, as a view names them.
    assert_eq!(queue["subjects"][0]["actor"]["did"], bob.did.as_str());

    // A rule the group's, cited.
    let rule = format!("{}/{did}/{NS}.rule/3mzjdd6rnfise", meta_space(&did));
    let body = json!({"group": did, "subject": alice.did, "reasonType": "rude", "rule": rule});
    let (status, said) = call(&h, &bob, "createReport", body).await;
    assert_eq!(status, 200, "{said}");
    let (_, history) = query(
        &h,
        &alice,
        "getModSubjectHistory",
        &format!("group={did}&subject={}", alice.did),
    )
    .await;
    let event = &history["events"][0];
    assert_eq!(event["id"], said["id"]);
    assert_eq!(event["createdBy"], bob.did.as_str());
    assert_eq!(event["creator"]["did"], bob.did.as_str());
    assert_eq!(event["event"]["$type"], "fyi.opensocial.defs#reportEvent");
    assert_eq!(event["event"]["reasonType"], "rude");
    assert_eq!(event["event"]["rule"], rule);
    // The account in the queue, by its DID.
    let (_, queue) = query(&h, &alice, "listModSubjects", &format!("group={did}")).await;
    assert_eq!(queue["subjects"][0]["subject"], alice.did.as_str());
    assert_eq!(queue["subjects"][0]["actor"]["did"], alice.did.as_str());

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
    for body in [
        about(json!({"group": did}), &subject),
        about(
            json!({"group": did, "reasonType": "x".repeat(257)}),
            &subject,
        ),
        about(
            json!({"group": did, "reasonType": "spam", "reason": "x".repeat(20_001)}),
            &subject,
        ),
        about(
            json!({"group": did, "reasonType": "spam", "rule": format!("{space}/{did}/{NS}.rule/x")}),
            &subject,
        ),
        about(
            json!({"group": did, "reasonType": "spam", "rule": format!("at://{did}/{NS}.rule/x")}),
            &subject,
        ),
    ] {
        let said = call(&h, &bob, "createReport", body.clone()).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{body} {}", said.1);
    }
    let lxm = format!("{NS}.createReport");
    let body = about(json!({"group": did, "reasonType": "spam"}), &subject);
    assert_eq!(common::groups::post(&h, &lxm, &body, &[]).await.0, 401);
}

/// Puts `labeler` as the group's, as it would write it.
async fn put_labeler(h: &Running, did: &str, labeler: Json) {
    let write = Write::Update {
        collection: format!("{NS}.labeler"),
        rkey: "self".into(),
        value: labeler,
    };
    h.host
        .space_write(did, &meta_space(did), &[write])
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
        "reasonTypes": ["spam"],
        "subjectTypes": ["record"],
        "subjectCollections": ["com.example.event"],
    });
    put_labeler(&h, &did, labeler).await;
    assert_eq!(report(&h, &bob, &did, record(uri)).await.0, 200);
    let rude = about(json!({"group": did, "reasonType": "rude"}), &record(uri));
    assert_eq!(
        error(&call(&h, &bob, "createReport", rude).await),
        (400, "InvalidRequest")
    );
    for subject in [account(&alice.did), note] {
        assert_eq!(
            error(&report(&h, &bob, &did, subject.clone()).await),
            (400, "InvalidSubject"),
            "{subject}"
        );
    }
    // An appeal is a reason like any other, and this labeler takes none.
    assert_eq!(
        error(&appeal(&h, &bob, &did, record(uri)).await),
        (400, "InvalidRequest")
    );
}

#[tokio::test]
async fn takes_an_appeal_only_from_its_subject() {
    let h = start(&[], false).await;
    let (did, _, post, [alice, bob, carol]) = group(&h).await;
    // A new group's labeler takes every reason, appeals among them, as
    // plain words.
    let labeler = h
        .host
        .group_record(&did, &meta_space(&did), &format!("{NS}.labeler"), "self")
        .unwrap()
        .unwrap();
    assert_eq!(
        labeler["reasonTypes"],
        json!([
            "spam",
            "violation",
            "misleading",
            "nsfw",
            "rude",
            "other",
            APPEAL
        ])
    );
    assert!(labeler.get("createdAt").is_none(), "{labeler}");

    // Bob appeals his post in the calendar, and his public one: each a
    // report in the queue, its reason an appeal.
    let in_space = post["uri"].as_str().unwrap().to_string();
    let public = format!("at://{}/app.bsky.feed.post/3mzjdd6rnfise", bob.did);
    for uri in [&in_space, &public] {
        let (status, said) = appeal(&h, &bob, &did, record(uri)).await;
        assert_eq!(status, 200, "{said}");
        let q = format!("group={did}&subject={uri}");
        let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
        assert_eq!(kinds(&history), ["reportEvent"]);
        assert_eq!(history["events"][0]["event"]["reasonType"], APPEAL);
        assert_eq!(history["events"][0]["createdBy"], bob.did.as_str());
        // Alice reads it, but it is not hers to appeal.
        let said = appeal(&h, &alice, &did, record(uri)).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{uri}");
        let message = said.1["message"].as_str().unwrap();
        assert!(message.contains("only its subject may appeal"), "{message}");
    }
    let resolve = json!({"group": did, "subject": public, "status": "resolved"});
    assert_eq!(call(&h, &alice, "resolveModSubject", resolve).await.0, 200);

    // Bob appeals for his own account; no one else may for him.
    let (status, said) = appeal(&h, &bob, &did, account(&bob.did)).await;
    assert_eq!(status, 200, "{said}");
    let said = appeal(&h, &alice, &did, account(&bob.did)).await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    // Carol was never in the group, and the queue has nothing of hers.
    let said = appeal(&h, &carol, &did, account(&carol.did)).await;
    assert_eq!(error(&said), (400, "InvalidSubject"));

    // Put out, Bob still appeals what the queue has of his.
    let app = App::new("console");
    let members = members_space(&did);
    let token = app.token_for(&h, &alice, &did, &members).await;
    let body = json!({"repo": did, "space": members, "collection": format!("{NS}.membership"), "rkey": bob.did});
    let said = app
        .call(&h, "com.atproto.space.deleteRecord", &token, Some(&body))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    for subject in [account(&bob.did), record(&in_space), record(&public)] {
        let (status, said) = appeal(&h, &bob, &did, subject.clone()).await;
        assert_eq!(status, 200, "{subject} {said}");
    }
    let q = format!("group={did}&subject={}", bob.did);
    let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(
        kinds(&history),
        ["reportEvent", "ejectEvent", "reportEvent"]
    );
    // The appeal reopened what was resolved.
    let (_, queue) = query(&h, &alice, "listModSubjects", &format!("group={did}")).await;
    let open = queue["subjects"].as_array().unwrap();
    assert!(open.iter().any(|s| s["subject"] == public), "{queue}");
    // But not a post of his the queue never had, nor in a group where it
    // has nothing of his.
    let other = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let unseen = format!("at://{}/app.bsky.feed.post/3mzjdd6rnfisf", bob.did);
    for (group, subject) in [
        (&did, record(&unseen)),
        (&other, account(&bob.did)),
        (&other, record(&public)),
    ] {
        let said = appeal(&h, &bob, group, subject.clone()).await;
        assert_eq!(error(&said), (400, "InvalidSubject"), "{group} {subject}");
    }
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
    let (_, queue) = query(&h, &alice, "listModSubjects", &format!("group={did}")).await;
    assert_eq!(queue["subjects"][0]["subject"], uri);
    assert_eq!(queue["subjects"][0]["reportCount"], 0);

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
    let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(kinds(&history), ["labelEvent", "labelEvent"]);
    let events = &history["events"];
    assert_eq!(events[0]["event"]["uri"], made["uri"]);
    assert_eq!(events[0]["event"]["val"], "spam");
    assert_eq!(events[0]["event"]["deleted"], false);
    assert_eq!(events[1]["event"]["uri"], made["uri"]);
    assert_eq!(events[1]["event"]["deleted"], true);
    assert_eq!(events[1]["createdBy"], alice.did.as_str());
    // About the version it named.
    let q = format!("group={did}&subject={uri}&cid={CID}");
    let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(kinds(&history), ["labelEvent", "labelEvent"]);

    // An account's label goes in the members space, whoever they are.
    let members = members_space(&did);
    let (status, made) = labelled(&h, &alice, &did, &members, label(&carol.did, "troll")).await;
    assert_eq!(status, 200, "{made}");
    let q = format!("group={did}&subject={}", carol.did);
    let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(history["events"][0]["event"]["val"], "troll");
    assert_eq!(history["events"][0]["event"]["uri"], made["uri"]);
    assert_eq!(history["events"][0]["createdBy"], alice.did.as_str());

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
    let rule = format!("{meta}/{did}/{NS}.rule/r");
    let (status, said) = labelled(&h, &bob, &did, &meta, label(&rule, "x")).await;
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
    let policy = json!({"$type": "fyi.opensocial.policy", "grants": [
        {"role": "member", "read": true},
        {"role": "moderator", "read": true, "scopes": [every]},
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
    let (_, queue) = query(&h, &alice, "listModSubjects", &format!("group={did}")).await;
    assert_eq!(queue["subjects"].as_array().unwrap().len(), 1);
    assert_eq!(queue["subjects"][0]["subject"], uri);
    assert_eq!(queue["subjects"][0]["reportCount"], 1);
    assert_eq!(queue["subjects"][0]["actor"]["did"], bob.did.as_str());
    // Its own subject, not its author's.
    let q = format!("group={did}&subject={}", bob.did);
    let (_, theirs) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(theirs["events"], json!([]));
    let q = format!("group={did}&subject={uri}&cid={CID}");
    let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(kinds(&history), ["reportEvent"]);
    assert_eq!(history["events"][0]["cid"], CID);
    let resolve = json!({"group": did, "subject": uri, "status": "resolved"});
    let (status, view) = call(&h, &alice, "resolveModSubject", resolve).await;
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
    let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(
        kinds(&history),
        [
            "reportEvent",
            "statusEvent",
            "reportEvent",
            "labelEvent",
            "labelEvent"
        ]
    );
    let events = &history["events"];
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
async fn resolves_and_escalates_with_a_note() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, _]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    report(&h, &bob, &did, record(uri)).await;
    let resolve = |status: &str, note: Option<&str>| {
        let mut body = json!({"group": did, "subject": uri, "status": status});
        if let Some(n) = note {
            body["note"] = json!(n);
        }
        body
    };
    let (status, view) = call(&h, &alice, "resolveModSubject", resolve("escalated", None)).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["status"], "escalated");
    assert_eq!(view["subject"], uri);
    assert_eq!(view["reportCount"], 1);
    assert!(view["actor"]["did"].is_string());
    // Setting the status it has keeps a note.
    let (_, view) = call(
        &h,
        &alice,
        "resolveModSubject",
        resolve("escalated", Some("Asked them")),
    )
    .await;
    assert_eq!(view["status"], "escalated");
    let (_, view) = call(&h, &alice, "resolveModSubject", resolve("resolved", None)).await;
    assert_eq!(view["status"], "resolved");
    // A label leaves it resolved; a new report opens it again.
    let mut spam = label(uri, "spam");
    spam["cid"] = json!(CID);
    labelled(&h, &alice, &did, &space, spam).await;
    let listed = async |status: &str| {
        let q = format!("group={did}&status={status}");
        query(&h, &alice, "listModSubjects", &q).await.1["subjects"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(listed("resolved").await, 1);
    report(&h, &bob, &did, version(uri, CID)).await;
    assert_eq!(listed("open").await, 1);

    let history = async |q: &str| {
        let q = format!("group={did}&subject={uri}{q}");
        query(&h, &alice, "getModSubjectHistory", &q).await
    };
    let (_, all) = history("").await;
    assert_eq!(
        kinds(&all),
        [
            "reportEvent",
            "statusEvent",
            "statusEvent",
            "statusEvent",
            "labelEvent",
            "reportEvent"
        ]
    );
    assert_eq!(all["events"][0]["event"]["reasonType"], "spam");
    assert_eq!(all["events"][0]["event"]["reason"], "ads");
    assert_eq!(all["events"][2]["event"]["status"], "escalated");
    assert_eq!(all["events"][2]["event"]["note"], "Asked them");
    assert!(all["events"][1]["event"].get("note").is_none());
    assert!(all.get("cursor").is_none());
    // Narrowed to one version of the record: what was said of it.
    let (_, version) = history(&format!("&cid={CID}")).await;
    assert_eq!(kinds(&version), ["labelEvent", "reportEvent"]);
    // Each names the version it is about, for a moderator to act on it.
    assert_eq!(version["events"][0]["cid"], CID);
    assert!(all["events"][2].get("cid").is_none());
    // A page at a time, oldest first.
    let mut seen = Vec::new();
    let mut cursor = String::new();
    loop {
        let (_, page) = history(&format!("&limit=4{cursor}")).await;
        seen.extend(kinds(&page));
        match page["cursor"].as_str() {
            Some(c) => cursor = format!("&cursor={c}"),
            None => break,
        }
    }
    assert_eq!(seen, kinds(&all));
    assert_eq!(
        error(&history("&cursor=soon").await),
        (400, "InvalidCursor")
    );
    // An account nobody has said anything of has no history.
    let q = format!("group={did}&subject={}", bob.did);
    let (status, none) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!((status, none["events"].clone()), (200, json!([])));
    let q = format!("group={did}&subject=at://nothing");
    let said = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(error(&said), (400, "InvalidSubject"));

    let long = "x".repeat(2001);
    let never = format!("{uri}2");
    for (who, body, refused) in [
        (&alice, resolve("open", None), (400, "InvalidRequest")),
        (
            &alice,
            resolve("resolved", Some(&long)),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "subject": never, "status": "resolved"}),
            (400, "SubjectNotFound"),
        ),
        (
            &alice,
            json!({"group": did, "subject": format!("at://did:plc:other/space/{CALENDAR}/rides/{}/c.d.e/f", bob.did), "status": "resolved"}),
            (400, "InvalidSubject"),
        ),
        (
            &alice,
            json!({"group": did, "subject": "not a uri", "status": "resolved"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "status": "resolved"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"subject": uri, "status": "resolved"}),
            (400, "InvalidRequest"),
        ),
        (&bob, resolve("resolved", None), (403, "Forbidden")),
    ] {
        assert_eq!(
            error(&call(&h, who, "resolveModSubject", body.clone()).await),
            refused,
            "{body}"
        );
    }
}

#[tokio::test]
async fn lists_the_queue_by_status_a_page_at_a_time() {
    let h = start(&[], false).await;
    let (did, space, _, [alice, bob, _]) = group(&h).await;
    let subjects: Vec<String> = (0..3)
        .map(|i| format!("{space}/{}/com.example.event/r{i}", bob.did))
        .collect();
    for s in &subjects {
        report(&h, &bob, &did, record(s)).await;
    }
    let body = json!({"group": did, "subject": subjects[0], "status": "resolved"});
    call(&h, &alice, "resolveModSubject", body).await;
    let listed = async |q: String| {
        query(&h, &alice, "listModSubjects", &format!("group={did}{q}"))
            .await
            .1
    };
    // Open, unless asked.
    let open = listed(String::new()).await;
    assert_eq!(open["subjects"].as_array().unwrap().len(), 2);
    let resolved = listed("&status=resolved".into()).await;
    assert_eq!(resolved["subjects"][0]["subject"], subjects[0]);

    let mut seen = Vec::new();
    let mut cursor = String::new();
    loop {
        let page = listed(format!("&limit=1{cursor}")).await;
        let got = page["subjects"].as_array().unwrap();
        seen.extend(
            got.iter()
                .map(|s| s["subject"].as_str().unwrap().to_string()),
        );
        match page["cursor"].as_str() {
            Some(c) => cursor = format!("&cursor={}", urlencode(c)),
            None => break,
        }
    }
    seen.sort();
    assert_eq!(seen, subjects[1..]);

    for (q, refused) in [
        ("&cursor=soon", "InvalidCursor"),
        ("&status=closed", "InvalidRequest"),
    ] {
        let said = query(&h, &alice, "listModSubjects", &format!("group={did}{q}")).await;
        assert_eq!(error(&said), (400, refused), "{q}");
    }
    assert_eq!(
        error(&query(&h, &bob, "listModSubjects", &format!("group={did}")).await),
        (403, "Forbidden")
    );
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
        reason: Some("spam".into()),
        created_at: 1,
        ..Event::default()
    };
    h.host.store.moderate(&did, &carol.did, &event).unwrap();
    let q = format!("group={did}&subject={}", carol.did);
    let (_, history) = query(&h, &alice, "getModSubjectHistory", &q).await;
    assert_eq!(
        history["events"][0]["event"],
        json!({"$type": "fyi.opensocial.defs#ejectEvent"})
    );
}

#[test]
fn keeps_what_was_resolved_escalated_and_noted_as_a_status_set() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.sqlite");
    drop(Store::open(&path).unwrap());
    // As a database from before: its schema back to just before the
    // migration that made events statuses, then events of each kind.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    let added = group_host::store::MIGRATIONS
        .iter()
        .position(|m| m.contains("ALTER TABLE mod_event ADD COLUMN cid"))
        .unwrap();
    common::rewind(&conn, added);
    for (subject, kind, note) in [
        ("did:plc:b", "report", None),
        ("did:plc:b", "escalate", None),
        ("did:plc:b", "note", Some("asked")),
        ("did:plc:b", "resolve", None),
        ("did:plc:b", "note", Some("done")),
        ("did:plc:c", "note", Some("first")),
    ] {
        conn.execute(
            "INSERT INTO mod_event (grp, subject, kind, note, created_at) VALUES ('did:plc:a', ?1, ?2, ?3, 0)",
            rusqlite::params![subject, kind, note],
        )
        .unwrap();
    }
    drop(conn);
    let store = Store::open(&path).unwrap();
    let said = |subject: &str| {
        let events = store.events("did:plc:a", subject, None, 0, 10).unwrap();
        let said = events
            .iter()
            .map(|e| (e.kind.clone(), e.status.clone(), e.note.clone()));
        said.collect::<Vec<_>>()
    };
    let status = |s: &str, n: Option<&str>| {
        (
            "status".to_string(),
            Some(s.to_string()),
            n.map(String::from),
        )
    };
    assert_eq!(
        said("did:plc:b"),
        [
            ("report".to_string(), None, None),
            status("escalated", None),
            status("escalated", Some("asked")),
            status("resolved", None),
            status("resolved", Some("done")),
        ]
    );
    assert_eq!(said("did:plc:c"), [status("open", Some("first"))]);
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
    let meta = meta_space(&did);
    let labeler = h
        .host
        .group_record(&did, &meta, "fyi.opensocial.labeler", "self");
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
        .group_record(&did, &meta, "fyi.opensocial.labeler", "self");
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
    let lxm = format!("{NS}.listSpaceRecords");
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
    let blob = format!("{NS}.getSpaceBlob");
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
    let lxm = format!("{NS}.listSpaceRecords");
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
    let q = format!("group={did}&subject={}", urlencode(subject));
    query(h, who, "getModSubjectHistory", &q).await
}

/// The file `cid` of the record `subject`, as `who` reviews it.
async fn reviewed_file(h: &Running, who: &Authority, did: &str, subject: &str, cid: &str) -> Read {
    let lxm = format!("{NS}.getModSubjectBlob");
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
        let lxm = format!("{NS}.getSpaceBlob");
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
    let role = json!({"$type": format!("{NS}.role"), "displayName": "Moderator", "scopes": [format!("rpc:{NS}.listModSubjects?aud=*")], "createdAt": "2026-11-03T00:00:00Z"});
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
