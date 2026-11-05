//! A group's face, written as the group by those who may configure it:
//! its profile, its pictures and its rules, records in its meta space,
//! and its declaration, which follows who may read that space.

mod common;

use common::groups::*;
use common::oauth::*;
use common::spaces::*;
use common::start;
use group_host::group::{members_space, meta_space};
use group_host::repo::Write;
use serde_json::{Value as Json, json};

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

/// The console, writing as the group in its meta space as Alice, with
/// her token.
async fn console(h: &common::Running, did: &str, alice: &Authority) -> (App, String) {
    let app = App::new("console");
    let token = app.token_for(h, alice, did, &meta_space(did)).await;
    (app, token)
}

/// A profile named Riders, joined by asking, with `extra` beside.
fn riders(extra: Json) -> Json {
    let mut p = json!({"$type": format!("{NS}.profile"), "displayName": "Riders", "joinPolicy": "approval"});
    p.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    p
}

/// What uploading `bytes` as `mime` answered: the blob.
async fn uploaded(h: &common::Running, app: &App, token: &str, bytes: Vec<u8>, mime: &str) -> Json {
    let said = app.upload(h, token, bytes, mime).await;
    assert_eq!(said.status, 200, "{}", said.body);
    said.body["blob"].clone()
}

#[tokio::test]
async fn an_admin_changes_the_profile() {
    let (h, did, [alice, _, _]) = group().await;
    let (app, token) = console(&h, &did, &alice).await;
    let meta = meta_space(&did);
    // As large as a profile's picture may be.
    let avatar = uploaded(&h, &app, &token, vec![7; 2_000_000], "image/webp").await;
    assert_eq!(avatar["size"], 2_000_000);
    assert_eq!(avatar["mimeType"], "image/webp");
    let banner = uploaded(&h, &app, &token, b"road".to_vec(), "image/jpeg").await;
    // As large as a link's icon may be.
    let icon = uploaded(&h, &app, &token, vec![8; 100_000], "image/png").await;
    let record = riders(json!({
        "displayName": "Riders United",
        "description": "We ride.",
        "url": "https://riders.example",
        // As many links as a profile holds, a title as long as one may be.
        "links": [
            {"url": "https://chat.riders.example", "title": "Our chat", "icon": icon},
            {"url": "http://calendar.riders.example/rides"},
            {"url": "https://a.example", "title": "x".repeat(64)},
            {"url": "https://b.example", "title": "B"},
            {"url": "https://c.example", "title": "C"},
        ],
        "joinPolicy": "invite",
        "location": {"$type": "community.lexicon.location.address", "locality": "Ljubljana", "country": "SI"},
        "online": true,
        "topics": [{"topic": "bikes"}, {"topic": "x".repeat(64)}],
        "languages": [{"language": "sl"}, {"language": "en-GB"}],
        "avatar": avatar,
        "banner": banner,
    }));
    let said = app
        .put(
            &h,
            &token,
            &meta,
            &format!("{NS}.profile"),
            "self",
            record.clone(),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["uri"], format!("{meta}/{did}/{NS}.profile/self"));
    assert_eq!(said.body["cid"].as_str().map(str::len), Some(59));
    assert_eq!(profile(&h, &did), record);
    // getGroup shows it as it is kept.
    let shown = view(&h, &did, Some(&alice)).await;
    assert_eq!(shown["group"]["displayName"], "Riders United");
    assert_eq!(shown["group"]["joinPolicy"], "invite");
    assert_eq!(shown["group"]["links"], record["links"]);

    // Replaced whole: what is left out goes, pictures and all.
    let said = app
        .put(
            &h,
            &token,
            &meta,
            &format!("{NS}.profile"),
            "self",
            riders(json!({})),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let p = profile(&h, &did);
    assert_eq!(p.get("avatar"), None);
    assert_eq!(p.get("topics"), None);
    assert_eq!(p["joinPolicy"], "approval");
}

#[tokio::test]
async fn refuses_a_profile_it_should_not_keep() {
    let (h, did, [alice, bob, carol]) = group().await;
    let (app, token) = console(&h, &did, &alice).await;
    let meta = meta_space(&did);
    let words = h.host.upload_blob(&did, b"words", "text/plain").unwrap();
    let moving = uploaded(&h, &app, &token, b"GIF89a".to_vec(), "image/gif").await;
    let large = uploaded(&h, &app, &token, vec![0; 2_000_001], "image/png").await;
    let face = uploaded(&h, &app, &token, b"face".to_vec(), "image/webp").await;
    let wide = uploaded(&h, &app, &token, vec![0; 100_001], "image/png").await;
    // Its type and size are the upload's, whatever the record says.
    let mut retyped = face.clone();
    retyped["mimeType"] = json!("image/png");
    let mut resized = face.clone();
    resized["size"] = json!(999);
    let never = json!({"$type": "blob", "ref": {"$link": "bafkrgqaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}, "mimeType": "image/png", "size": 1});
    let anothers = json!({"$type": "blob", "ref": {"$link": "bafkreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm"}, "mimeType": "image/png", "size": 4});
    for record in [
        riders(json!({"displayName": "  "})),
        riders(json!({"joinPolicy": "sometimes"})),
        // A join policy is one of the words the lexicon knows.
        riders(json!({"joinPolicy": "fyi.opensocial.profile#invite"})),
        riders(json!({"location": {"$type": "xyz.place"}})),
        riders(json!({"url": "ftp://riders.example"})),
        riders(
            json!({"links": [{"url": "https://riders.example"}, {"url": "ftp://riders.example"}]}),
        ),
        riders(json!({"links": [{"title": "Nowhere"}]})),
        riders(json!({"links": [{"url": "https://riders.example", "title": "x".repeat(65)}]})),
        riders(json!({"links": vec![json!({"url": "https://riders.example"}); 6]})),
        riders(json!({"topics": [{"topic": "bikes"}, {"topic": " bikes "}]})),
        riders(json!({"topics": [{"topic": " "}]})),
        riders(json!({"languages": [{"language": "sl"}, {"language": "sl"}]})),
        // Pictures that are not the group's own images, as it kept them.
        riders(json!({"avatar": words})),
        riders(json!({"avatar": moving})),
        riders(json!({"banner": large})),
        riders(json!({"avatar": retyped})),
        riders(json!({"avatar": resized})),
        riders(json!({"avatar": never})),
        riders(json!({"avatar": anothers})),
        riders(json!({"avatar": {"ref": "x"}})),
        // Icons that are not the group's own small PNGs, as it kept them.
        riders(json!({"links": [{"url": "https://riders.example", "icon": face}]})),
        riders(json!({"links": [{"url": "https://riders.example", "icon": wide}]})),
        riders(json!({"links": [{"url": "https://riders.example", "icon": anothers}]})),
        // Longer than the lexicon has them, or not of its format.
        riders(json!({"displayName": "x".repeat(65)})),
        riders(json!({"topics": [{"topic": "x".repeat(65)}]})),
        riders(json!({"languages": [{"language": "not a tag"}]})),
        riders(json!({"description": "x".repeat(2001)})),
        json!({"$type": format!("{NS}.profile"), "displayName": "Riders"}),
    ] {
        let said = app
            .put(
                &h,
                &token,
                &meta,
                &format!("{NS}.profile"),
                "self",
                record.clone(),
            )
            .await;
        assert_eq!(said.error(), (400, "InvalidRequest"), "{record}");
    }
    // Kept at self, and never taken away.
    let said = app
        .put(
            &h,
            &token,
            &meta,
            &format!("{NS}.profile"),
            "other",
            riders(json!({})),
        )
        .await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
    let body =
        json!({"repo": did, "space": meta, "collection": format!("{NS}.profile"), "rkey": "self"});
    let said = app
        .call(&h, "com.atproto.space.deleteRecord", &token, Some(&body))
        .await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
    // Nothing refused was kept.
    assert_eq!(profile(&h, &did)["displayName"], "Riders");

    // Who may not configure the group gets no token to write it with.
    for who in [&bob, &carol] {
        let said = group_token(&h, &App::new("console"), who, &did, &meta).await;
        assert_eq!(said.error(), (403, "Forbidden"));
    }
    // And no token at all, nothing.
    let body = json!({"repo": did, "space": meta, "collection": format!("{NS}.profile"), "rkey": "self", "record": riders(json!({}))});
    let said = post(&h, "com.atproto.space.putRecord", &body, &[]).await;
    assert_eq!(said.0, 401);
}

#[tokio::test]
async fn an_admin_keeps_the_rules() {
    let (h, did, [alice, _, _]) = group().await;
    let (app, token) = console(&h, &did, &alice).await;
    let meta = meta_space(&did);
    let collection = format!("{NS}.rule");
    let rule = |key: &str| h.host.group_record(&did, &meta, &collection, key).unwrap();
    let make = async |record: Json| {
        let body = json!({"repo": did, "space": meta, "collection": collection, "record": record});
        app.call(&h, "com.atproto.space.createRecord", &token, Some(&body))
            .await
    };
    let key = |said: &Said| {
        let uri = said.body["uri"].as_str().unwrap();
        uri.rsplit('/').next().unwrap().to_string()
    };
    // A new rule is keyed by the host's clock, a TID.
    let first =
        make(json!({"$type": collection, "title": "Be kind", "text": "Always.", "order": 0})).await;
    assert_eq!(first.status, 200, "{}", first.body);
    let first_key = key(&first);
    assert_eq!(first_key.len(), 13);
    let second = make(json!({"$type": collection, "title": "No spam", "text": ""})).await;
    let second_key = key(&second);
    let third =
        make(json!({"$type": collection, "title": "Ride safe", "text": "", "order": 1})).await;
    let third_key = key(&third);
    // Shown by their order, those with none last.
    let shown = view(&h, &did, Some(&alice)).await;
    let keys: Vec<&str> = shown["group"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["rkey"].as_str().unwrap())
        .collect();
    assert_eq!(keys, [&first_key, &third_key, &second_key]);

    // Changed in place, it keeps its key, so a label citing it still does.
    let changed =
        json!({"$type": collection, "title": "Be very kind", "text": "Always.", "order": 5});
    let said = app
        .put(&h, &token, &meta, &collection, &first_key, changed.clone())
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["uri"], first.body["uri"]);
    assert_eq!(rule(&first_key).unwrap(), changed);

    // Taken away, it is gone; taking it away again is no matter.
    let body = json!({"repo": did, "space": meta, "collection": collection, "rkey": second_key});
    for _ in 0..2 {
        let said = app
            .call(&h, "com.atproto.space.deleteRecord", &token, Some(&body))
            .await;
        assert_eq!(said.status, 200, "{}", said.body);
    }
    assert_eq!(rule(&second_key), None);

    let at = async |rkey: &str, record: Json| {
        app.put(&h, &token, &meta, &collection, rkey, record).await
    };
    for (rkey, record) in [
        // A key that is not the clock's: a rule is cited by its TID.
        (
            "chosen",
            json!({"$type": collection, "title": "Chosen", "text": ""}),
        ),
        (
            first_key.as_str(),
            json!({"$type": collection, "title": " ", "text": ""}),
        ),
        (
            first_key.as_str(),
            json!({"$type": collection, "title": "Odd", "text": "", "order": -1}),
        ),
        (
            first_key.as_str(),
            json!({"$type": collection, "title": "Odd"}),
        ),
        // Longer than the lexicon has them.
        (
            first_key.as_str(),
            json!({"$type": collection, "title": "x".repeat(65), "text": ""}),
        ),
        (
            first_key.as_str(),
            json!({"$type": collection, "title": "Odd", "text": "x".repeat(2001)}),
        ),
    ] {
        let said = at(rkey, record.clone()).await;
        assert_eq!(said.error(), (400, "InvalidRequest"), "{record}");
    }
    assert_eq!(rule("chosen"), None);
    assert_eq!(rule(&first_key).unwrap(), changed);
}

#[tokio::test]
async fn declares_the_group_while_anyone_may_read_its_meta() {
    let (h, did, [alice, _, _]) = group().await;
    // An app signed in as the group, Alice approving it.
    let app = App::loopback(&format!("atproto repo:{NS}.declaration"));
    let tokens = app.sign_in(&h, &alice, &did).await;
    let token = tokens["access_token"].as_str().unwrap().to_string();
    let collection = format!("{NS}.declaration");
    let declared = || {
        h.host
            .store
            .record(&did, &collection, "self")
            .unwrap()
            .is_some()
    };
    let put = async |rkey: &str, record: Json| {
        let body = json!({"repo": did, "collection": collection, "rkey": rkey, "record": record});
        app.call(&h, "com.atproto.repo.putRecord", &token, Some(&body))
            .await
    };
    let declaration = json!({"$type": collection, "meta": meta_space(&did)});
    assert!(declared());
    // Taken away, the group is unlisted; put back, listed again.
    let body = json!({"repo": did, "collection": collection, "rkey": "self"});
    let said = app
        .call(&h, "com.atproto.repo.deleteRecord", &token, Some(&body))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert!(!declared());
    let said = put("self", declaration.clone()).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert!(declared());
    // It names the group's own meta, at self.
    let elsewhere = json!({"$type": collection, "meta": "at://did:plc:elsewhere/space/fyi.opensocial.meta/self"});
    assert_eq!(
        put("self", elsewhere).await.error(),
        (400, "InvalidRequest")
    );
    let said = put("other", declaration.clone()).await;
    assert_eq!(said.error(), (400, "InvalidRequest"));

    // Once only members read its meta, it is unlisted, and stays so.
    let meta = meta_space(&did);
    let hide = json!({"group": did, "space": meta, "access": {"grants": [{"role": "member", "scopes": ["space:*?action=read"]}]}});
    assert_eq!(call(&h, &alice, "updateSpace", hide).await.0, 200);
    assert!(!declared());
    assert_eq!(
        put("self", declaration).await.error(),
        (400, "InvalidRequest")
    );
    assert!(!declared());
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
    let (app, token) = console(&h, &did, &alice).await;
    let avatar = uploaded(&h, &app, &token, b"face".to_vec(), "image/png").await;
    let record = riders(json!({"avatar": avatar}));
    let collection = format!("{NS}.profile");
    let said = app
        .put(&h, &token, &meta_space(&did), &collection, "self", record)
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
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
    let photo = uploaded(&h, &app, &token, b"ride".to_vec(), "image/jpeg").await;
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
    let away = uploaded(&h, &app, &token, b"away".to_vec(), "image/png").await;
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
