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
                value: json!({"$type": format!("{NS}.membership"), "member": bob.did, "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}),
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
    // As large as a profile's picture may be.
    let face = vec![7; 2_000_000];
    let (status, image) = upload(&h, &alice, &did, face, "image/webp").await;
    assert_eq!(status, 200, "{image}");
    let avatar = image["blob"].clone();
    assert_eq!(avatar["size"], 2_000_000);
    assert_eq!(avatar["mimeType"], "image/webp");
    // Its type and size are the upload's, whatever the caller says of them.
    let mut said = avatar.clone();
    said["mimeType"] = json!("image/png");
    said["size"] = json!(999);
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
            "topics": [{"topic": "bikes"}, {"topic": " bikes "}, {"topic": ""}, {"topic": "x".repeat(64)}],
            "languages": [{"language": "sl"}, {"language": "en-GB"}],
            "avatar": said,
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
    assert_eq!(p["avatar"], avatar);
    assert_eq!(p["createdAt"], created);
    assert_eq!(put["cid"].as_str().map(str::len), Some(59));

    // A replace: what finds it and its site, left out, are none; the
    // images are kept; a policy named is taken.
    let (status, _) = call(
        &h,
        &alice,
        "updateProfile",
        json!({"group": did, "displayName": "Riders", "joinPolicy": "invite"}),
    )
    .await;
    assert_eq!(status, 200);
    let p = profile(&h, &did);
    assert_eq!(p.get("topics"), None);
    assert_eq!(p.get("location"), None);
    assert_eq!(p.get("description"), None);
    assert_eq!(p.get("url"), None);
    assert_eq!(p["avatar"]["ref"], avatar["ref"]);
    assert_eq!(p["joinPolicy"], "invite");
    // And removeAvatar clears one.
    call(
        &h,
        &alice,
        "updateProfile",
        json!({"group": did, "displayName": "Riders", "removeAvatar": true}),
    )
    .await;
    assert_eq!(profile(&h, &did).get("avatar"), None);
}

#[tokio::test]
async fn refuses_a_profile_it_should_not_keep() {
    let (h, did, [alice, bob, carol]) = group().await;
    let words = h.host.upload_blob(&did, b"words", "text/plain").unwrap();
    for (who, body, refused) in [
        // A blob of the group's that is no image.
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "avatar": words}),
            (400, "InvalidRequest"),
        ),
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
        // A join policy is one of the words the lexicon knows.
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "joinPolicy": "fyi.opensocial.profile#invite"}),
            (400, "InvalidRequest"),
        ),
        // An image whose CID the host never keeps.
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "avatar": {"$type": "blob", "ref": {"$link": "bafkrgqaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}, "mimeType": "image/png", "size": 1}}),
            (400, "InvalidRequest"),
        ),
        // Longer than the lexicon has them, or not of its format.
        (
            &alice,
            json!({"group": did, "displayName": "x".repeat(65)}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "topics": [{"topic": "x".repeat(65)}]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "languages": [{"language": "not a tag"}]}),
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            json!({"group": did, "displayName": "Riders", "description": "x".repeat(2001)}),
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

    for (who, bytes, mime, refused) in [
        (&bob, b"face".to_vec(), "image/png", (403, "Forbidden")),
        (&alice, vec![], "image/png", (400, "InvalidRequest")),
        // A picture of a kind a profile does not take.
        (
            &alice,
            b"GIF89a".to_vec(),
            "image/gif",
            (400, "InvalidRequest"),
        ),
        (
            &alice,
            vec![0; 2_000_001],
            "image/png",
            (400, "InvalidRequest"),
        ),
        // No image a page is given.
        (
            &alice,
            b"page".to_vec(),
            "application/pdf",
            (400, "InvalidRequest"),
        ),
    ] {
        let said = upload(&h, who, &did, bytes, mime).await;
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
    // A new one goes after it.
    let (_, third) = call(
        &h,
        &alice,
        "putRule",
        json!({"group": did, "title": "Chosen"}),
    )
    .await;
    let third_key = third["uri"].as_str().unwrap().rsplit('/').next().unwrap();
    assert_eq!(rule(third_key).unwrap()["order"], 6);
    // A key no rule has makes none: a new rule's key is the host's.
    let said = call(
        &h,
        &alice,
        "putRule",
        json!({"group": did, "rkey": "chosen", "title": "Chosen"}),
    )
    .await;
    assert_eq!(said.0, 400, "{}", said.1);
    assert_eq!(rule("chosen"), None);

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
        // Longer than the lexicon has them.
        ("putRule", json!({"group": did, "title": "x".repeat(101)})),
        (
            "putRule",
            json!({"group": did, "title": "Odd", "text": "x".repeat(2001)}),
        ),
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

/// An image of `did`'s, `cid`, as a page asks for it, as `who` if anyone:
/// its status, its headers, and its bytes.
async fn image(
    h: &common::Running,
    did: &str,
    cid: &str,
    who: Option<&Authority>,
) -> (u16, reqwest::header::HeaderMap, Vec<u8>) {
    let mut req = h.http.get(format!("{}/img/{did}/{cid}", h.url));
    if let Some(who) = who {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let cookie = h.host.session_cookie(&who.did, now);
        req = req.header(
            "cookie",
            format!("{}={cookie}", group_host::console_login::COOKIE),
        );
    }
    let res = req.send().await.unwrap();
    let status = res.status().as_u16();
    let headers = res.headers().clone();
    (status, headers, res.bytes().await.unwrap().to_vec())
}

/// A record holding `blob`, of a collection of its own.
fn holding(rkey: &str, blob: &Json) -> Write {
    Write::Create {
        collection: "com.example.photo".into(),
        rkey: rkey.into(),
        value: json!({"$type": "com.example.photo", "image": blob, "createdAt": "2026-11-04T00:00:00Z"}),
    }
}

#[tokio::test]
async fn shows_an_image_to_whoever_may_see_a_record_that_holds_it() {
    let (h, did, [alice, bob, carol]) = group().await;
    let cid = |b: &Json| b["ref"]["$link"].as_str().unwrap().to_string();

    // The avatar of a group listed on the network: anyone's to see who is
    // signed in, as an image and nothing more, kept for good; no one's who
    // is not.
    let (_, up) = upload(&h, &alice, &did, b"face".to_vec(), "image/png").await;
    let avatar = up["blob"].clone();
    let body = json!({"group": did, "displayName": "Riders", "avatar": avatar});
    assert_eq!(call(&h, &alice, "updateProfile", body).await.0, 200);
    assert_eq!(image(&h, &did, &cid(&avatar), None).await.0, 404);
    let (status, headers, bytes) = image(&h, &did, &cid(&avatar), Some(&carol)).await;
    assert_eq!((status, bytes.as_slice()), (200, &b"face"[..]));
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(
        headers["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    assert_eq!(
        headers["cache-control"],
        "private, max-age=31536000, immutable"
    );

    // Unlisted, its profile is its readers' alone: a member's, not a
    // stranger's.
    let meta = meta_space(&did);
    let hide = json!({"group": did, "space": meta, "access": {"grants": [{"role": "member", "scopes": ["space:*?action=read"]}]}});
    assert_eq!(call(&h, &alice, "updateSpace", hide).await.0, 200);
    assert_eq!(image(&h, &did, &cid(&avatar), None).await.0, 404);
    assert_eq!(image(&h, &did, &cid(&avatar), Some(&carol)).await.0, 404);
    assert_eq!(image(&h, &did, &cid(&avatar), Some(&bob)).await.0, 200);

    // In its public repo: anyone's.
    let (_, up) = upload(&h, &alice, &did, b"ride".to_vec(), "image/jpeg").await;
    let photo = up["blob"].clone();
    h.host.write(&did, &[holding("ride", &photo)]).unwrap();
    assert_eq!(image(&h, &did, &cid(&photo), None).await.0, 200);

    // A file that is no image is not served as one, wherever it is held.
    let text = h.host.upload_blob(&did, b"words", "text/plain").unwrap();
    h.host.write(&did, &[holding("words", &text)]).unwrap();
    assert_eq!(image(&h, &did, &cid(&text), None).await.0, 404);

    // Nothing of its held by no record, and no CID at all.
    let loose = h.host.upload_blob(&did, b"loose", "image/png").unwrap();
    assert_eq!(image(&h, &did, &cid(&loose), None).await.0, 404);
    assert_eq!(image(&h, &did, "not-a-cid", None).await.0, 404);

    // In a space of another's, which this host does not govern: no one's.
    let elsewhere = "at://did:plc:elsewhere/space/com.example.forum/main";
    let (_, up) = upload(&h, &alice, &did, b"away".to_vec(), "image/png").await;
    let away = up["blob"].clone();
    h.host
        .space_write(&did, elsewhere, &[holding("away", &away)])
        .await
        .unwrap();
    assert_eq!(image(&h, &did, &cid(&away), Some(&bob)).await.0, 404);

    // Switched off, it shows nothing.
    h.host
        .store
        .deactivate(&did, 0, &alice.did, &[], &[])
        .unwrap();
    assert_eq!(image(&h, &did, &cid(&photo), None).await.0, 404);
}
