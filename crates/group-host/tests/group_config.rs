//! A group's face changed by those who may configure it: its profile, its
//! images and its rules.

mod common;

use common::groups::*;
use common::spaces::*;
use common::start;
use group_host::group::{members_space, meta_space};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

const NS: &str = "fyi.opensocial";

/// A group Alice started, Bob a plain member of it, and Carol a stranger.
async fn group() -> (common::Running, String, [Authority; 3]) {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let did = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders", "joinPolicy": "open"}),
    )
    .await;
    h.host
        .space_write(
            &did,
            &members_space(&did),
            &[Write::Create {
                collection: format!("{NS}.membership"),
                rkey: bob.did.clone(),
                value: json!({"$type": format!("{NS}.membership"), "member": bob.did, "roles": [{"role": "member"}], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    (h, did, [alice, bob, carol])
}

async fn call(h: &common::Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

fn profile(h: &common::Running, did: &str) -> Json {
    h.host
        .group_record(did, &meta_space(did), &format!("{NS}.profile"), "self")
        .unwrap()
        .unwrap()
}

async fn upload(
    h: &common::Running,
    who: &Authority,
    did: &str,
    bytes: Vec<u8>,
    mime: &str,
) -> (u16, Json) {
    let lxm = format!("{NS}.uploadImage");
    let (k, v) = auth(h, who, &lxm);
    let res = h
        .http
        .post(format!("{}/xrpc/{lxm}?group={did}", h.url))
        .header(k, v)
        .header("content-type", mime)
        .body(bytes)
        .send()
        .await
        .unwrap();
    (
        res.status().as_u16(),
        res.json().await.unwrap_or(Json::Null),
    )
}

#[tokio::test]
async fn an_admin_changes_the_profile() {
    let (h, did, [alice, _, _]) = group().await;
    let created = profile(&h, &did)["createdAt"].clone();
    let (status, image) = upload(&h, &alice, &did, b"face".to_vec(), "image/png").await;
    assert_eq!(status, 200, "{image}");
    let avatar = image["blob"].clone();
    assert_eq!(avatar["mimeType"], "image/png");
    let (status, put) = call(
        &h,
        &alice,
        "updateProfile",
        json!({
            "group": did,
            "displayName": "  Riders United ",
            "description": "We ride.",
            "url": "https://riders.example",
            "location": {"$type": "community.lexicon.location.address", "locality": "Ljubljana", "country": "SI"},
            "online": true,
            "topics": [{"topic": "bikes"}, {"topic": " bikes "}, {"topic": ""}, {"topic": "x".repeat(80)}],
            "languages": [{"language": "sl"}, {"language": "en-GB"}, {"language": "not a tag"}],
            "avatar": avatar,
        }),
    )
    .await;
    assert_eq!(status, 200, "{put}");
    assert_eq!(
        put["uri"],
        format!("{}/{did}/{NS}.profile/self", meta_space(&did))
    );
    let p = profile(&h, &did);
    assert_eq!(p["displayName"], "Riders United");
    assert_eq!(p["description"], "We ride.");
    // Its join policy is kept when it is not named.
    assert_eq!(p["joinPolicy"], "open");
    assert_eq!(p["online"], true);
    assert_eq!(
        p["topics"],
        json!([{"topic": "bikes"}, {"topic": "x".repeat(64)}])
    );
    assert_eq!(
        p["languages"],
        json!([{"language": "sl"}, {"language": "en-GB"}])
    );
    assert_eq!(p["avatar"]["ref"], avatar["ref"]);
    assert_eq!(p["createdAt"], created);
    assert_eq!(put["cid"].as_str().map(str::len), Some(59));

    // A replace: what finds it, left out, is none; the images and site
    // are kept; a policy named is taken.
    let (status, _) = call(
        &h,
        &alice,
        "updateProfile",
        json!({"group": did, "displayName": "Riders", "joinPolicy": "fyi.opensocial.profile#invite"}),
    )
    .await;
    assert_eq!(status, 200);
    let p = profile(&h, &did);
    assert_eq!(p.get("topics"), None);
    assert_eq!(p.get("location"), None);
    assert_eq!(p.get("description"), None);
    assert_eq!(p["url"], "https://riders.example");
    assert_eq!(p["avatar"]["ref"], avatar["ref"]);
    assert_eq!(p["joinPolicy"], "invite");
    // And `null` clears an image.
    call(
        &h,
        &alice,
        "updateProfile",
        json!({"group": did, "displayName": "Riders", "avatar": null}),
    )
    .await;
    assert_eq!(profile(&h, &did).get("avatar"), None);
}

#[tokio::test]
async fn refuses_a_profile_it_should_not_keep() {
    let (h, did, [alice, bob, carol]) = group().await;
    for (who, body, refused) in [
        (
            &bob,
            json!({"group": did, "displayName": "Mine"}),
            (403, "Forbidden"),
        ),
        (
            &carol,
            json!({"group": did, "displayName": "Mine"}),
            (403, "Forbidden"),
        ),
        (
            &alice,
            json!({"displayName": "Riders"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": "did:plc:nobody", "displayName": "Riders"}),
            (400, "GroupNotFound"),
        ),
        (
            &alice,
            json!({"group": did, "displayName": "  "}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "joinPolicy": "sometimes"}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "location": {"$type": "xyz.place"}}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "avatar": {"ref": "x"}}),
            (400, "InvalidRequest"),
        ),
        // An image that is not the group's.
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "avatar": {"$type": "blob", "ref": {"$link": "bafkreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm"}, "mimeType": "image/png", "size": 4}}),
            (400, "InvalidRequest"),
        ),
    ] {
        let said = call(&h, who, "updateProfile", body.clone()).await;
        assert_eq!(
            (said.0, said.1["error"].as_str().unwrap()),
            refused,
            "{body}"
        );
    }
    let lxm = format!("{NS}.updateProfile");
    let said = post(&h, &lxm, &json!({"group": did, "displayName": "X"}), &[]).await;
    assert_eq!(said.0, 401);

    for (who, bytes, refused) in [
        (&bob, b"face".to_vec(), (403, "Forbidden")),
        (&alice, vec![], (400, "InvalidRequest")),
        (&alice, vec![0; 1_000_001], (400, "InvalidRequest")),
    ] {
        let said = upload(&h, who, &did, bytes, "image/png").await;
        assert_eq!((said.0, said.1["error"].as_str().unwrap()), refused);
    }
    let lxm = format!("{NS}.uploadImage");
    let said = read(&h, &lxm, "", &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(said.status, 405);
}

#[tokio::test]
async fn an_admin_keeps_the_rules() {
    let (h, did, [alice, bob, _]) = group().await;
    let rule = |key: &str| {
        h.host
            .group_record(&did, &meta_space(&did), &format!("{NS}.rule"), key)
            .unwrap()
    };
    let (status, first) = call(
        &h,
        &alice,
        "putRule",
        json!({"group": did, "title": "Be kind", "text": " Always. "}),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    let first_key = first["uri"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    let (_, second) = call(
        &h,
        &alice,
        "putRule",
        json!({"group": did, "title": "No spam"}),
    )
    .await;
    let second_key = second["uri"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    // A new rule goes last.
    assert_eq!(rule(&first_key).unwrap()["order"], 0);
    assert_eq!(rule(&first_key).unwrap()["text"], "Always.");
    assert_eq!(rule(&second_key).unwrap()["order"], 1);

    // Changed in place, it keeps its key, its place and when it was made.
    let made = rule(&first_key).unwrap()["createdAt"].clone();
    let (_, again) = call(
        &h,
        &alice,
        "putRule",
        json!({"group": did, "rkey": first_key, "title": "Be very kind"}),
    )
    .await;
    assert_eq!(again["uri"], first["uri"]);
    let changed = rule(&first_key).unwrap();
    assert_eq!(
        (changed["title"].clone(), changed["order"].clone()),
        (json!("Be very kind"), json!(0))
    );
    assert_eq!(changed["createdAt"], made);
    // Or moved.
    call(
        &h,
        &alice,
        "putRule",
        json!({"group": did, "rkey": first_key, "title": "Be very kind", "order": 5}),
    )
    .await;
    assert_eq!(rule(&first_key).unwrap()["order"], 5);
    // Or put where it is named.
    call(
        &h,
        &alice,
        "putRule",
        json!({"group": did, "rkey": "chosen", "title": "Chosen"}),
    )
    .await;
    assert_eq!(rule("chosen").unwrap()["order"], 6);

    let (status, _) = call(
        &h,
        &alice,
        "deleteRule",
        json!({"group": did, "rkey": second_key}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(rule(&second_key), None);
    // Taking away one that is not there is no matter.
    let (status, _) = call(
        &h,
        &alice,
        "deleteRule",
        json!({"group": did, "rkey": second_key}),
    )
    .await;
    assert_eq!(status, 200);

    for (method, body, refused) in [
        (
            "putRule",
            json!({"group": did, "title": "Mine"}),
            (403, "Forbidden"),
        ),
        (
            "deleteRule",
            json!({"group": did, "rkey": "chosen"}),
            (403, "Forbidden"),
        ),
    ] {
        let said = call(&h, &bob, method, body).await;
        assert_eq!((said.0, said.1["error"].as_str().unwrap()), refused);
    }
    for (method, body) in [
        ("putRule", json!({"group": did, "title": " "})),
        (
            "putRule",
            json!({"group": did, "title": "Odd", "order": -1}),
        ),
        // A key no record could have.
        (
            "putRule",
            json!({"group": did, "rkey": "a/b", "title": "Odd"}),
        ),
        ("putRule", json!({"title": "Odd"})),
        ("deleteRule", json!({"group": did})),
        ("deleteRule", json!({"rkey": "x"})),
    ] {
        let said = call(&h, &alice, method, body.clone()).await;
        assert_eq!(
            (said.0, said.1["error"].as_str().unwrap()),
            (400, "InvalidRequest"),
            "{body}"
        );
    }
}
