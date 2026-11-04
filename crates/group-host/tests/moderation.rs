//! A group's moderation: reports from anyone, labels written beside what
//! they label, and the queue between.

mod common;

use common::groups::*;
use common::spaces::*;
use common::{Running, start};
use group_host::group::{members_space, meta_space};
use group_host::moderation::space_of;
use group_host::repo::{Write, record_cid};
use serde_json::{Value as Json, json};

const CALENDAR: &str = "com.example.calendar";

async fn membership(h: &Running, did: &str, who: &str, roles: &[&str]) {
    let roles: Vec<Json> = roles.iter().map(|r| json!({"role": r})).collect();
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

async fn report(h: &Running, who: &Authority, subject: Json) -> (u16, Json) {
    let lxm = "com.atproto.moderation.createReport";
    let body = json!({"reasonType": "com.atproto.moderation.defs#reasonSpam", "reason": "ads", "subject": subject});
    post(h, lxm, &body, &[auth(h, who, lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// Riders, Alice's, with a calendar, Bob a member and Carol not; and a
/// post in the calendar, by its URI and CID.
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
        json!({"group": did, "spaceType": CALENDAR, "skey": "rides", "readableBy": [{"role": "member"}]}),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    membership(h, &did, &bob.did, &["member"]).await;
    let space = made["uri"].as_str().unwrap().to_string();
    let post = json!({"uri": format!("{space}/{}/com.example.event/ride", bob.did), "cid": "bafyreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm"});
    (did, space, post, [alice, bob, carol])
}

#[tokio::test]
async fn takes_a_report_from_anyone() {
    let h = start(&[], false).await;
    let (did, _, record, [alice, _, carol]) = group(&h).await;
    let (status, said) = report(&h, &carol, record.clone()).await;
    assert_eq!(status, 200, "{said}");
    assert_eq!(said["reportedBy"], carol.did.as_str());
    assert_eq!(said["reason"], "ads");
    assert_eq!(said["subject"], record);
    // Each report its own id, however close together they come.
    let (_, again) = report(&h, &carol, record.clone()).await;
    let ids = (said["id"].as_i64().unwrap(), again["id"].as_i64().unwrap());
    assert_eq!(ids.1, ids.0 + 1, "{ids:?}");
    let (_, queue) = query(&h, &alice, "listSubjects", &format!("group={did}")).await;
    assert_eq!(queue["subjects"][0]["subject"], record["uri"]);
    assert_eq!(queue["subjects"][0]["status"], "open");
    assert_eq!(queue["subjects"][0]["reportCount"], 2);
    assert!(queue["subjects"][0]["lastReportedAt"].is_string());

    let other = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let elsewhere = json!({"uri": format!("at://did:plc:elsewhere/space/{CALENDAR}/rides/{}/c.d.e/f", carol.did), "cid": "x"});
    let not_ours = json!({"uri": format!("at://{other}/space/{CALENDAR}/none/{}/c.d.e/f", carol.did), "cid": "x"});
    for (subject, refused) in [
        (json!({"did": carol.did}), (400, "InvalidRequest")),
        (
            json!({"uri": format!("at://{did}/app.bsky.feed.post/x"), "cid": "x"}),
            (400, "InvalidRequest"),
        ),
        (elsewhere, (400, "NotHosted")),
        (not_ours, (400, "NotHosted")),
        // Each part of its URI what it says, so none is unbounded.
        (
            json!({"uri": format!("at://{did}/space/{CALENDAR}/rides/{}/c.d.e/{}", carol.did, "r".repeat(513)), "cid": "x"}),
            (400, "InvalidRequest"),
        ),
    ] {
        assert_eq!(
            error(&report(&h, &carol, subject.clone()).await),
            refused,
            "{subject}"
        );
    }
    let lxm = "com.atproto.moderation.createReport";
    let said = post(
        &h,
        lxm,
        &json!({"subject": record}),
        &[auth(&h, &carol, lxm)],
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidRequest"));
    for body in [
        json!({"subject": record, "reasonType": "x".repeat(257)}),
        json!({"subject": record, "reasonType": "com.atproto.moderation.defs#reasonSpam", "reason": "x".repeat(20_001)}),
    ] {
        let said = post(&h, lxm, &body, &[auth(&h, &carol, lxm)]).await;
        assert_eq!(error(&said), (400, "InvalidRequest"), "{}", said.1);
    }
    assert_eq!(post(&h, lxm, &json!({}), &[]).await.0, 401);
}

#[tokio::test]
async fn writes_a_label_beside_what_it_labels() {
    let h = start(&[], false).await;
    let (did, space, record, [alice, bob, carol]) = group(&h).await;
    let rule = format!("{}/{did}/{NS}.rule/3mzjdd6rnfise", meta_space(&did));
    let (status, made) = call(
        &h,
        &alice,
        "applyLabel",
        json!({"group": did, "subject": record, "val": "spam", "rule": rule}),
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
    assert_eq!(label["subject"]["uri"], record["uri"]);
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
        json!({"group": did, "subject": record, "val": "spam"}),
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

    // An account's label goes in the members space, whoever they are.
    let (status, made) = call(
        &h,
        &alice,
        "applyLabel",
        json!({"group": did, "subject": {"did": carol.did}, "val": "troll"}),
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
    assert_eq!(history["events"][0]["kind"], "label");
    assert_eq!(history["events"][0]["val"], "troll");
    assert_eq!(history["events"][0]["actor"], alice.did.as_str());

    let other = start_group(
        &h,
        &alice,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let cid = record["cid"].clone();
    let theirs =
        json!({"uri": format!("at://{other}/space/{CALENDAR}/x/{}/c.d.e/f", bob.did), "cid": cid});
    for (body, refused) in [
        (
            json!({"group": did, "subject": {"did": "not a did"}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": {"uri": record["uri"]}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": {"uri": format!("at://{did}/a.b.c/x"), "cid": cid}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        // A CID that is none.
        (
            json!({"group": did, "subject": {"uri": record["uri"], "cid": "x"}, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        // A value longer than the lexicon has it, in characters or bytes.
        (
            json!({"group": did, "subject": record, "val": "x".repeat(129)}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": record, "val": "\u{e9}".repeat(100)}),
            (400, "InvalidRequest"),
        ),
        // A rule that is no record's URI.
        (
            json!({"group": did, "subject": record, "val": "x", "rule": "at://x/y/z"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": did, "subject": theirs, "val": "x"}),
            (403, "Forbidden"),
        ),
        (
            json!({"group": did, "subject": record, "val": ""}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"subject": record, "val": "x"}),
            (400, "InvalidRequest"),
        ),
        (
            json!({"group": "did:plc:nothing", "subject": record, "val": "x"}),
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
    let (did, _, record, [alice, bob, _]) = group(&h).await;
    let label = |val: &str| json!({"group": did, "subject": record, "val": val});
    assert_eq!(
        error(&call(&h, &bob, "applyLabel", label("spam")).await),
        (403, "Forbidden")
    );
    let (status, said) = call(
        &h,
        &alice,
        "putRole",
        json!({"group": did, "role": "labeler", "name": "Labeler", "actions": ["label"]}),
    )
    .await;
    assert_eq!(status, 200, "{said}");
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
async fn resolves_escalates_and_notes() {
    let h = start(&[], false).await;
    let (did, _, record, [alice, bob, carol]) = group(&h).await;
    report(&h, &carol, record.clone()).await;
    let uri = record["uri"].as_str().unwrap();
    let resolve = |action: &str, note: Option<&str>| {
        let mut body = json!({"group": did, "subject": uri, "action": action});
        if let Some(n) = note {
            body["note"] = json!(n);
        }
        body
    };
    let status = async || {
        let (_, history) = query(
            &h,
            &alice,
            "getSubjectHistory",
            &format!("group={did}&subject={uri}"),
        )
        .await;
        history["subject"]["status"].as_str().unwrap().to_string()
    };
    assert_eq!(
        call(&h, &alice, "resolveSubject", resolve("escalate", None))
            .await
            .0,
        200
    );
    assert_eq!(status().await, "escalated");
    assert_eq!(
        call(
            &h,
            &alice,
            "resolveSubject",
            resolve("note", Some("Asked them"))
        )
        .await
        .0,
        200
    );
    assert_eq!(status().await, "escalated");
    assert_eq!(
        call(&h, &alice, "resolveSubject", resolve("resolve", None))
            .await
            .0,
        200
    );
    assert_eq!(status().await, "resolved");
    // A label leaves it resolved; a new report opens it again.
    call(
        &h,
        &alice,
        "applyLabel",
        json!({"group": did, "subject": record, "val": "spam"}),
    )
    .await;
    assert_eq!(status().await, "resolved");
    report(&h, &carol, record.clone()).await;
    assert_eq!(status().await, "open");

    let (_, history) = query(
        &h,
        &alice,
        "getSubjectHistory",
        &format!("group={did}&subject={uri}"),
    )
    .await;
    let kinds: Vec<&str> = history["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["report", "escalate", "note", "resolve", "label", "report"]
    );
    assert_eq!(
        history["events"][0]["reasonType"],
        "com.atproto.moderation.defs#reasonSpam"
    );
    assert_eq!(history["events"][0]["reason"], "ads");
    assert_eq!(history["events"][2]["note"], "Asked them");

    // No action is no action it knows.
    let said = call(
        &h,
        &alice,
        "resolveSubject",
        json!({"group": did, "subject": uri}),
    )
    .await;
    assert_eq!(said.1["message"], "action is required");
    let long = "x".repeat(301);
    for (who, body, refused) in [
        (&alice, resolve("shrug", None), (400, "InvalidRequest")),
        (
            &alice,
            resolve("note", Some(&long)),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "subject": "at://nothing", "action": "resolve"}),
            (400, "SubjectNotFound"),
        ),
        (
            &alice,
            json!({"group": did, "action": "resolve"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"subject": uri, "action": "resolve"}),
            (400, "InvalidRequest"),
        ),
        (&bob, resolve("resolve", None), (403, "Forbidden")),
    ] {
        assert_eq!(
            error(&call(&h, who, "resolveSubject", body.clone()).await),
            refused,
            "{body}"
        );
    }
    let said = query(
        &h,
        &alice,
        "getSubjectHistory",
        &format!("group={did}&subject=at://nothing"),
    )
    .await;
    assert_eq!(error(&said), (400, "SubjectNotFound"));
}

#[tokio::test]
async fn lists_the_queue_by_status_a_page_at_a_time() {
    let h = start(&[], false).await;
    let (did, space, _, [alice, bob, carol]) = group(&h).await;
    let subjects: Vec<String> = (0..3)
        .map(|i| format!("{space}/{}/com.example.event/r{i}", bob.did))
        .collect();
    for s in &subjects {
        report(&h, &carol, json!({"uri": s, "cid": "x"})).await;
    }
    let body = json!({"group": did, "subject": subjects[0], "action": "resolve"});
    call(&h, &alice, "resolveSubject", body).await;
    let listed = async |q: String| {
        query(&h, &alice, "listSubjects", &format!("group={did}{q}"))
            .await
            .1
    };
    let open = listed("&status=open".into()).await;
    assert_eq!(open["subjects"].as_array().unwrap().len(), 2);
    let resolved = listed("&status=resolved".into()).await;
    assert_eq!(resolved["subjects"][0]["subject"], subjects[0].as_str());

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
    let mut all = subjects.clone();
    all.sort();
    assert_eq!(seen, all);

    assert_eq!(
        error(
            &query(
                &h,
                &alice,
                "listSubjects",
                &format!("group={did}&cursor=soon")
            )
            .await
        ),
        (400, "InvalidRequest")
    );
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
