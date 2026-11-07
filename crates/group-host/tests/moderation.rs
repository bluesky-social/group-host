//! A group's moderation: reports from those who can read what they
//! report, labels written beside what they label, and the queue between.

mod common;

use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::{members_space, meta_space};
use group_host::moderation::{Event, space_of};
use group_host::repo::{Write, record_cid};
use group_host::store::Store;
use serde_json::{Value as Json, json};

const CALENDAR: &str = "com.example.calendar";
const ACCOUNT: &str = "fyi.opensocial.defs#accountSubject";
const RECORD: &str = "fyi.opensocial.defs#recordSubject";
const CID: &str = "bafyreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm";

async fn membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let collection = format!("{NS}.membership");
    let value = json!({"$type": collection, "member": who, "roles": roles, "createdAt": "2026-11-03T00:00:00Z"});
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

async fn report(h: &Running, who: &Authority, group: &str, subject: Json) -> (u16, Json) {
    let body = json!({"group": group, "reasonType": "com.atproto.moderation.defs#reasonSpam", "reason": "ads", "subject": subject});
    call(h, who, "createReport", body).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

fn record(uri: &str) -> Json {
    json!({"$type": RECORD, "uri": uri})
}

fn account(did: &str) -> Json {
    json!({"$type": ACCOUNT, "did": did})
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
    let (status, made) = call(
        h,
        &alice,
        "createSpace",
        json!({"group": did, "type": CALENDAR, "skey": "rides", "access": {"grants": [{"role": "member", "scopes": ["space:*?action=read"]}]}}),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    membership(h, &did, &bob.did, &["member"]).await;
    let space = made["space"].as_str().unwrap().to_string();
    let post = json!({"$type": "com.atproto.repo.strongRef", "uri": format!("{space}/{}/com.example.event/ride", bob.did), "cid": CID});
    (did, space, post, [alice, bob, carol])
}

#[tokio::test]
async fn takes_a_report_from_one_who_reads_it() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, carol]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    let subject = json!({"$type": RECORD, "uri": uri, "cid": CID});
    let (status, said) = report(&h, &bob, &did, subject.clone()).await;
    assert_eq!(status, 200, "{said}");
    assert!(said["createdAt"].is_string());
    // Each report its own id, however close together they come.
    let (_, again) = report(&h, &bob, &did, subject.clone()).await;
    assert_ne!(said["id"], again["id"]);
    assert!(said["id"].is_string());
    let (_, queue) = query(&h, &alice, "listSubjects", &format!("group={did}")).await;
    assert_eq!(queue["subjects"][0]["subject"], record(uri));
    assert_eq!(queue["subjects"][0]["status"], "open");
    assert_eq!(queue["subjects"][0]["reportCount"], 2);
    // Whose record it is, as a view names them.
    assert_eq!(queue["subjects"][0]["actor"]["did"], bob.did.as_str());

    // A rule the group's, cited.
    let rule = format!("{}/{did}/{NS}.rule/3mzjdd6rnfise", meta_space(&did));
    let body = json!({"group": did, "subject": account(&alice.did), "reasonType": "com.atproto.moderation.defs#reasonRude", "rule": rule});
    let (status, said) = call(&h, &bob, "createReport", body).await;
    assert_eq!(status, 200, "{said}");
    let (_, history) = query(
        &h,
        &alice,
        "getSubjectHistory",
        &format!("group={did}&subject={}", alice.did),
    )
    .await;
    let event = &history["events"][0];
    assert_eq!(event["id"], said["id"]);
    assert_eq!(event["createdBy"], bob.did.as_str());
    assert_eq!(event["creator"]["did"], bob.did.as_str());
    assert_eq!(event["event"]["$type"], "fyi.opensocial.defs#reportEvent");
    assert_eq!(
        event["event"]["reasonType"],
        "com.atproto.moderation.defs#reasonRude"
    );
    assert_eq!(event["event"]["rule"], rule);
    // The account in the queue, as the union it was sent as.
    let (_, queue) = query(&h, &alice, "listSubjects", &format!("group={did}")).await;
    assert_eq!(queue["subjects"][0]["subject"], account(&alice.did));
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
            (400, "InvalidRequest"),
        ),
        // A subject of a kind the open union does not list.
        (
            &bob,
            json!({"$type": "com.example.thing", "did": bob.did}),
            (400, "InvalidSubject"),
        ),
        (&bob, json!({"did": bob.did}), (400, "InvalidRequest")),
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
        json!({"group": did, "subject": subject}),
        json!({"group": did, "subject": subject, "reasonType": "x".repeat(257)}),
        json!({"group": did, "subject": subject, "reasonType": "com.atproto.moderation.defs#reasonSpam", "reason": "x".repeat(20_001)}),
        json!({"group": did, "subject": subject, "reasonType": "com.atproto.moderation.defs#reasonSpam", "rule": format!("{space}/{did}/{NS}.rule/x")}),
        json!({"group": did, "subject": subject, "reasonType": "com.atproto.moderation.defs#reasonSpam", "rule": format!("at://{did}/{NS}.rule/x")}),
    ] {
        let said = call(&h, &bob, "createReport", body.clone()).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{body} {}", said.1);
    }
    let lxm = format!("{NS}.createReport");
    assert_eq!(common::groups::post(&h, &lxm, &json!({"group": did, "subject": subject, "reasonType": "com.atproto.moderation.defs#reasonSpam"}), &[]).await.0, 401);
}

#[tokio::test]
async fn takes_only_the_reports_its_labeler_does() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, _]) = group(&h).await;
    let labeler = json!({
        "$type": format!("{NS}.labeler"),
        "policies": {"labelValues": ["!hide"]},
        "reasonTypes": ["com.atproto.moderation.defs#reasonSpam"],
        "subjectTypes": ["record"],
        "subjectCollections": ["com.example.event"],
        "createdAt": "2026-11-04T00:00:00Z",
    });
    let write = Write::Update {
        collection: format!("{NS}.labeler"),
        rkey: "self".into(),
        value: labeler,
    };
    h.host
        .space_write(&did, &meta_space(&did), &[write])
        .await
        .unwrap();
    let uri = post["uri"].as_str().unwrap();
    assert_eq!(report(&h, &bob, &did, record(uri)).await.0, 200);
    let rude = json!({"group": did, "subject": record(uri), "reasonType": "com.atproto.moderation.defs#reasonRude"});
    assert_eq!(
        error(&call(&h, &bob, "createReport", rude).await),
        (400, "InvalidRequest")
    );
    let note = record(&format!("{space}/{}/com.example.note/n", bob.did));
    for subject in [account(&alice.did), note] {
        assert_eq!(
            error(&report(&h, &bob, &did, subject.clone()).await),
            (400, "InvalidSubject"),
            "{subject}"
        );
    }
}

#[tokio::test]
async fn writes_a_label_beside_what_it_labels() {
    let h = start(&[], false).await;
    let (did, space, post, [alice, bob, carol]) = group(&h).await;
    let rule = format!("{}/{did}/{NS}.rule/3mzjdd6rnfise", meta_space(&did));
    let (status, made) = call(
        &h,
        &alice,
        "applyLabel",
        json!({"group": did, "subject": post, "val": "spam", "rule": rule}),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let uri = made["uri"].as_str().unwrap();
    assert!(
        uri.starts_with(&format!("{space}/{did}/{NS}.label/")),
        "{uri}"
    );
    let rkey = uri.rsplit('/').next().unwrap();
    let label = h
        .host
        .group_record(&did, &space, &format!("{NS}.label"), rkey)
        .unwrap()
        .unwrap();
    assert_eq!(label["subject"]["uri"], post["uri"]);
    assert_eq!(label["subject"]["$type"], "com.atproto.repo.strongRef");
    assert_eq!(label["val"], "spam");
    assert_eq!(label["rule"], rule);
    // A label opens a subject, though nobody reported it.
    let (_, queue) = query(&h, &alice, "listSubjects", &format!("group={did}")).await;
    assert_eq!(queue["subjects"][0]["reportCount"], 0);

    // Taken back with a second record, not by deleting the first.
    let (status, negated) = call(
        &h,
        &alice,
        "negateLabel",
        json!({"group": did, "subject": post, "val": "spam"}),
    )
    .await;
    assert_eq!(status, 200, "{negated}");
    // Answered as a label is: the negation is a record of its own.
    let uri = negated["uri"].as_str().unwrap();
    assert!(
        uri.starts_with(&format!("{space}/{did}/{NS}.label/")),
        "{uri}"
    );
    assert_ne!(negated["uri"], made["uri"]);
    let rkey = uri.rsplit('/').next().unwrap();
    let negation = h
        .host
        .group_record(&did, &space, &format!("{NS}.label"), rkey)
        .unwrap()
        .unwrap();
    assert_eq!(negation["neg"], true);
    let cid = record_cid(&format!("{NS}.label"), &negation).unwrap();
    assert_eq!(negated["cid"], cid.to_string());
    let labels = h
        .host
        .group_records(&did, &space, &format!("{NS}.label"))
        .unwrap();
    assert_eq!(labels.len(), 2);
    assert!(labels.iter().any(|(_, l)| l["neg"] == true));
    // Each a label event, the negation the label taken back.
    let (_, history) = query(
        &h,
        &alice,
        "getSubjectHistory",
        &format!("group={did}&subject={}", post["uri"].as_str().unwrap()),
    )
    .await;
    assert_eq!(kinds(&history), ["labelEvent", "labelEvent"]);
    assert_eq!(history["events"][0]["event"]["uri"], made["uri"]);
    assert_eq!(history["events"][0]["event"]["deleted"], false);
    assert_eq!(history["events"][1]["event"]["uri"], negated["uri"]);
    assert_eq!(history["events"][1]["event"]["deleted"], true);

    // An account's label goes in the members space, whoever they are.
    let (status, made) = call(
        &h,
        &alice,
        "applyLabel",
        json!({"group": did, "subject": {"$type": "com.atproto.admin.defs#repoRef", "did": carol.did}, "val": "troll"}),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    assert!(
        made["uri"]
            .as_str()
            .unwrap()
            .starts_with(&members_space(&did))
    );
    let (_, history) = query(
        &h,
        &alice,
        "getSubjectHistory",
        &format!("group={did}&subject={}", carol.did),
    )
    .await;
    assert_eq!(history["events"][0]["event"]["val"], "troll");
    assert_eq!(history["events"][0]["createdBy"], alice.did.as_str());

    let other = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let cid = post["cid"].clone();
    let theirs = json!({"$type": "com.atproto.repo.strongRef", "uri": format!("at://{other}/space/{CALENDAR}/x/{}/c.d.e/f", bob.did), "cid": cid});
    for (body, refused) in [
        (
            json!({"group": did, "subject": {"$type": "com.atproto.admin.defs#repoRef", "did": "not a did"}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": {"$type": "com.atproto.repo.strongRef", "uri": post["uri"]}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": {"$type": "com.atproto.repo.strongRef", "uri": format!("at://{did}/a.b.c/x"), "cid": cid}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        // A CID that is none.
        (
            json!({"group": did, "subject": {"$type": "com.atproto.repo.strongRef", "uri": post["uri"], "cid": "bafynotacid"}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        // A subject of a kind the open union does not list.
        (
            json!({"group": did, "subject": {"$type": "com.example.thing", "did": "not a did"}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": {"$type": "com.example.thing"}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        // A value longer than the lexicon has it, in characters or bytes.
        (
            json!({"group": did, "subject": post, "val": "x".repeat(129)}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": post, "val": "\u{e9}".repeat(100)}),
            (400, "InvalidRequest"),
        ),
        // A rule that is no record's URI.
        (
            json!({"group": did, "subject": post, "val": "x", "rule": "at://did:plc:x/fyi.opensocial.rule/z"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": theirs, "val": "x"}),
            (403, "Forbidden"),
        ),
        (
            json!({"group": did, "subject": post, "val": ""}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"subject": post, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": "did:plc:nothing", "subject": post, "val": "x"}),
            (400, "GroupNotFound"),
        ),
    ] {
        assert_eq!(
            error(&call(&h, &alice, "applyLabel", body.clone()).await),
            refused,
            "{body}"
        );
    }
}

#[tokio::test]
async fn holds_each_label_to_the_action_it_needs() {
    let h = start(&[], false).await;
    let (did, _, post, [alice, bob, _]) = group(&h).await;
    let label = |val: &str| json!({"group": did, "subject": post, "val": val});
    assert_eq!(
        error(&call(&h, &bob, "applyLabel", label("spam")).await),
        (403, "Forbidden")
    );
    use common::members::{granting, keep, role};
    let labels = group_host::rbac::scope_of("label").unwrap();
    keep(&h, &did, "role", "labeler", role("Labeler")).await;
    let permissions = granting(&h, &did, "labeler", &[&labels], &[]);
    keep(&h, &did, "permissions", "self", permissions).await;
    membership(&h, &did, &bob.did, &["member", "labeler"]).await;
    assert_eq!(call(&h, &bob, "applyLabel", label("spam")).await.0, 200);
    // `!hide` and `!takedown` remove content: those take `takedown`.
    for val in ["!hide", "!takedown"] {
        assert_eq!(
            error(&call(&h, &bob, "applyLabel", label(val)).await),
            (403, "Forbidden"),
            "{val}"
        );
    }
    assert_eq!(
        call(&h, &alice, "applyLabel", label("!takedown")).await.0,
        200
    );
}

#[tokio::test]
async fn resolves_and_escalates_with_a_note() {
    let h = start(&[], false).await;
    let (did, _, post, [alice, bob, _]) = group(&h).await;
    let uri = post["uri"].as_str().unwrap();
    report(&h, &bob, &did, record(uri)).await;
    let resolve = |status: &str, note: Option<&str>| {
        let mut body = json!({"group": did, "subject": record(uri), "status": status});
        if let Some(n) = note {
            body["note"] = json!(n);
        }
        body
    };
    let (status, view) = call(&h, &alice, "resolveSubject", resolve("escalated", None)).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["status"], "escalated");
    assert_eq!(view["subject"], record(uri));
    assert_eq!(view["reportCount"], 1);
    assert!(view["actor"]["did"].is_string());
    // Setting the status it has keeps a note.
    let (_, view) = call(
        &h,
        &alice,
        "resolveSubject",
        resolve("escalated", Some("Asked them")),
    )
    .await;
    assert_eq!(view["status"], "escalated");
    let (_, view) = call(&h, &alice, "resolveSubject", resolve("resolved", None)).await;
    assert_eq!(view["status"], "resolved");
    // A label leaves it resolved; a new report opens it again.
    call(
        &h,
        &alice,
        "applyLabel",
        json!({"group": did, "subject": post, "val": "spam"}),
    )
    .await;
    let listed = async |status: &str| {
        let q = format!("group={did}&status={status}");
        query(&h, &alice, "listSubjects", &q).await.1["subjects"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(listed("resolved").await, 1);
    report(
        &h,
        &bob,
        &did,
        json!({"$type": RECORD, "uri": uri, "cid": CID}),
    )
    .await;
    assert_eq!(listed("open").await, 1);

    let history = async |q: &str| {
        let q = format!("group={did}&subject={uri}{q}");
        query(&h, &alice, "getSubjectHistory", &q).await
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
    assert_eq!(
        all["events"][0]["event"]["reasonType"],
        "com.atproto.moderation.defs#reasonSpam"
    );
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
        (400, "InvalidRequest")
    );
    // An account nobody has said anything of has no history.
    let q = format!("group={did}&subject={}", bob.did);
    let (status, none) = query(&h, &alice, "getSubjectHistory", &q).await;
    assert_eq!((status, none["events"].clone()), (200, json!([])));
    let q = format!("group={did}&subject=at://nothing");
    let said = query(&h, &alice, "getSubjectHistory", &q).await;
    assert_eq!(error(&said), (400, "InvalidSubject"));

    let long = "x".repeat(2001);
    let never = record(&format!("{uri}2"));
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
            json!({"group": did, "subject": record(&format!("at://{did}/app.bsky.feed.post/x")), "status": "resolved"}),
            (400, "InvalidSubject"),
        ),
        (
            &alice,
            json!({"group": did, "subject": uri, "status": "resolved"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "status": "resolved"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"subject": record(uri), "status": "resolved"}),
            (400, "InvalidRequest"),
        ),
        (&bob, resolve("resolved", None), (403, "Forbidden")),
    ] {
        assert_eq!(
            error(&call(&h, who, "resolveSubject", body.clone()).await),
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
    let body = json!({"group": did, "subject": record(&subjects[0]), "status": "resolved"});
    call(&h, &alice, "resolveSubject", body).await;
    let listed = async |q: String| {
        query(&h, &alice, "listSubjects", &format!("group={did}{q}"))
            .await
            .1
    };
    // Open, unless asked.
    let open = listed(String::new()).await;
    assert_eq!(open["subjects"].as_array().unwrap().len(), 2);
    let resolved = listed("&status=resolved".into()).await;
    assert_eq!(resolved["subjects"][0]["subject"], record(&subjects[0]));

    let mut seen = Vec::new();
    let mut cursor = String::new();
    loop {
        let page = listed(format!("&limit=1{cursor}")).await;
        let got = page["subjects"].as_array().unwrap();
        seen.extend(
            got.iter()
                .map(|s| s["subject"]["uri"].as_str().unwrap().to_string()),
        );
        match page["cursor"].as_str() {
            Some(c) => cursor = format!("&cursor={}", urlencode(c)),
            None => break,
        }
    }
    seen.sort();
    assert_eq!(seen, subjects[1..]);

    for q in ["&cursor=soon", "&status=closed"] {
        let said = query(&h, &alice, "listSubjects", &format!("group={did}{q}")).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{q}");
    }
    assert_eq!(
        error(&query(&h, &bob, "listSubjects", &format!("group={did}")).await),
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
    let (_, history) = query(&h, &alice, "getSubjectHistory", &q).await;
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
    // As a database from before: events of each kind, its schema a
    // version back.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    for column in ["cid", "rule", "uri", "status"] {
        let sql = format!("ALTER TABLE mod_event DROP COLUMN {column}");
        conn.execute(&sql, []).unwrap();
    }
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
    // Back to just before the migration that added them; those after it
    // run again on what they made.
    let migrations = group_host::store::MIGRATIONS;
    let added = migrations
        .iter()
        .position(|m| m.contains("ALTER TABLE mod_event ADD COLUMN cid"))
        .unwrap();
    // A table a later one renamed has its old name again, to be renamed.
    let lines = migrations[added + 1..].iter().flat_map(|m| m.lines());
    let renames = lines.filter_map(|l| {
        let l = l.trim().strip_prefix("ALTER TABLE ")?;
        l.trim_end_matches(';').split_once(" RENAME TO ")
    });
    for (from, to) in renames {
        let back = format!("ALTER TABLE {to} RENAME TO {from}");
        conn.execute(&back, []).unwrap();
    }
    conn.pragma_update(None, "user_version", added as i64)
        .unwrap();
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
