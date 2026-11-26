//! A group's face, written as the group by those who may configure it:
//! its about record and picture and its config, records in its meta
//! space; its rules and labeler, in its members space; and its
//! declaration, which follows who may read the meta space.

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
                value: json!({"$type": format!("{NS}.membership"), "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"}),
            }],
        )
        .await
        .unwrap();
    (h, did, [alice, bob, carol])
}

/// `did`'s own record of `collection` in `space`, at `rkey`, as kept.
fn kept(h: &common::Running, did: &str, space: &str, collection: &str, rkey: &str) -> Option<Json> {
    let collection = format!("{NS}.{collection}");
    h.host.group_record(did, space, &collection, rkey).unwrap()
}

fn about(h: &common::Running, did: &str) -> Json {
    kept(h, did, &meta_space(did), "about", "self").unwrap()
}

/// The console, writing as the group in `space` as Alice, with her token.
async fn console_in(
    h: &common::Running,
    did: &str,
    alice: &Authority,
    space: &str,
) -> (App, String) {
    let app = App::new("console");
    let token = app.token_for(h, alice, did, space).await;
    (app, token)
}

/// The console, writing as the group in its meta space as Alice, with
/// her token.
async fn console(h: &common::Running, did: &str, alice: &Authority) -> (App, String) {
    console_in(h, did, alice, &meta_space(did)).await
}

/// An about record naming the group Riders, with `extra` beside.
fn riders(extra: Json) -> Json {
    let mut p = json!({"$type": format!("{NS}.about"), "displayName": "Riders"});
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
async fn an_admin_changes_the_about_record_and_config() {
    let (h, did, [alice, bob, _]) = group().await;
    let (app, token) = console(&h, &did, &alice).await;
    let meta = meta_space(&did);
    // As large as the group's picture may be.
    let avatar = uploaded(&h, &app, &token, vec![7; 2_000_000], "image/webp").await;
    assert_eq!(avatar["size"], 2_000_000);
    assert_eq!(avatar["mimeType"], "image/webp");
    let record = riders(json!({
        "displayName": "Riders United",
        "description": "We ride.",
        "website": "https://riders.example",
        "avatar": avatar,
    }));
    let collection = format!("{NS}.about");
    let said = app
        .put(&h, &token, &meta, &collection, "self", record.clone())
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["uri"], format!("{meta}/{did}/{NS}.about/self"));
    assert_eq!(said.body["cid"].as_str().map(str::len), Some(59));
    assert_eq!(about(&h, &did), record);
    // How people join is the config's to say.
    let config = json!({"$type": format!("{NS}.config"), "joinPolicy": "invite"});
    let said = app
        .put(
            &h,
            &token,
            &meta,
            &format!("{NS}.config"),
            "self",
            config.clone(),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(kept(&h, &did, &meta, "config", "self").unwrap(), config);
    // getGroup shows them as they are kept, to whoever reads the meta.
    for who in [&alice, &bob] {
        let shown = view(&h, &did, Some(who)).await;
        assert_eq!(shown["group"]["displayName"], "Riders United");
        assert_eq!(shown["group"]["joinPolicy"], "invite");
        assert_eq!(shown["group"]["website"], "https://riders.example");
        assert_eq!(shown["group"]["avatar"], avatar["ref"]["$link"]);
    }
    // A group with no config yet, as one made before it, says no policy.
    let gone = Write::Delete {
        collection: format!("{NS}.config"),
        rkey: "self".into(),
    };
    h.host.space_write(&did, &meta, &[gone]).await.unwrap();
    let shown = view(&h, &did, Some(&alice)).await;
    assert_eq!(shown["group"].get("joinPolicy"), None);
    assert_eq!(shown["group"]["displayName"], "Riders United");

    // Replaced whole: what is left out goes, pictures and all.
    let said = app
        .put(&h, &token, &meta, &collection, "self", riders(json!({})))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    let p = about(&h, &did);
    assert_eq!(p.get("avatar"), None);
    assert_eq!(p.get("website"), None);
}

#[tokio::test]
async fn refuses_an_about_record_or_config_it_should_not_keep() {
    let (h, did, [alice, bob, carol]) = group().await;
    let (app, token) = console(&h, &did, &alice).await;
    let meta = meta_space(&did);
    let words = h.host.upload_blob(&did, b"words", "text/plain").unwrap();
    let moving = uploaded(&h, &app, &token, b"GIF89a".to_vec(), "image/gif").await;
    let large = uploaded(&h, &app, &token, vec![0; 2_000_001], "image/png").await;
    let face = uploaded(&h, &app, &token, b"face".to_vec(), "image/webp").await;
    // Its type and size are the upload's, whatever the record says.
    let mut retyped = face.clone();
    retyped["mimeType"] = json!("image/png");
    let mut resized = face.clone();
    resized["size"] = json!(999);
    let never = json!({"$type": "blob", "ref": {"$link": "bafkrgqaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}, "mimeType": "image/png", "size": 1});
    let anothers = json!({"$type": "blob", "ref": {"$link": "bafkreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm"}, "mimeType": "image/png", "size": 4});
    let collection = format!("{NS}.about");
    for record in [
        riders(json!({"displayName": "  "})),
        riders(json!({"website": "ftp://riders.example"})),
        // Pictures that are not the group's own images, as it kept them.
        riders(json!({"avatar": words})),
        riders(json!({"avatar": moving})),
        riders(json!({"avatar": large})),
        riders(json!({"avatar": retyped})),
        riders(json!({"avatar": resized})),
        riders(json!({"avatar": never})),
        riders(json!({"avatar": anothers})),
        riders(json!({"avatar": {"ref": "x"}})),
        // Longer than the lexicon has them, or not of its format.
        riders(json!({"displayName": "x".repeat(65)})),
        riders(json!({"description": "x".repeat(2001)})),
        json!({"$type": collection}),
    ] {
        let said = app
            .put(&h, &token, &meta, &collection, "self", record.clone())
            .await;
        assert_eq!(said.error(), (400, "InvalidRequest"), "{record}");
    }
    // A join policy is one of the words the lexicon knows.
    let config = format!("{NS}.config");
    for policy in ["sometimes", "org.atgrp.config#invite"] {
        let record = json!({"$type": config, "joinPolicy": policy});
        let said = app.put(&h, &token, &meta, &config, "self", record).await;
        assert_eq!(said.error(), (400, "InvalidRequest"), "{policy}");
    }
    let said = app
        .put(&h, &token, &meta, &config, "self", json!({"$type": config}))
        .await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
    // Kept at self, and never taken away.
    for (collection, record) in [
        (&collection, riders(json!({}))),
        (&config, json!({"$type": config, "joinPolicy": "open"})),
    ] {
        let said = app
            .put(&h, &token, &meta, collection, "other", record)
            .await;
        assert_eq!(said.error(), (400, "InvalidRequest"), "{collection}");
        let body = json!({"repo": did, "space": meta, "collection": collection, "rkey": "self"});
        let said = app
            .call(&h, "com.atproto.space.deleteRecord", &token, Some(&body))
            .await;
        assert_eq!(said.error(), (400, "InvalidRequest"), "{collection}");
    }
    // Nothing refused was kept.
    assert_eq!(about(&h, &did)["displayName"], "Riders");
    assert_eq!(
        kept(&h, &did, &meta, "config", "self").unwrap()["joinPolicy"],
        "open"
    );
    // Rules and the labeler are the members space's, not the meta's.
    let rule = json!({"$type": format!("{NS}.rule"), "title": "Be kind", "text": ""});
    let said = app
        .put(
            &h,
            &token,
            &meta,
            &format!("{NS}.rule"),
            "3jzfcijpj2z2a",
            rule,
        )
        .await;
    assert_eq!(said.status, 403, "{}", said.body);

    // Who may not configure the group gets no token to write it with.
    for who in [&bob, &carol] {
        let said = group_token(&h, &App::new("console"), who, &did, &meta).await;
        assert_eq!(said.error(), (403, "Forbidden"));
    }
    // And no token at all, nothing.
    let body = json!({"repo": did, "space": meta, "collection": collection, "rkey": "self", "record": riders(json!({}))});
    let said = post(&h, "com.atproto.space.putRecord", &body, &[]).await;
    assert_eq!(said.0, 401);
}

#[tokio::test]
async fn an_admin_keeps_the_rules_in_the_members_space() {
    let (h, did, [alice, bob, carol]) = group().await;
    let members = members_space(&did);
    let (app, token) = console_in(&h, &did, &alice, &members).await;
    let collection = format!("{NS}.rule");
    let rule = |key: &str| kept(&h, &did, &members, "rule", key);
    let make = async |record: Json| {
        let body =
            json!({"repo": did, "space": members, "collection": collection, "record": record});
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
    // Shown by their order, those with none last, to members.
    let shown = view(&h, &did, Some(&bob)).await;
    let keys: Vec<&str> = shown["group"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["rkey"].as_str().unwrap())
        .collect();
    assert_eq!(keys, [&first_key, &third_key, &second_key]);
    // Not to a stranger, who reads the meta space but not the members.
    let shown = view(&h, &did, Some(&carol)).await;
    assert_eq!(shown["group"]["displayName"], "Riders");
    assert_eq!(shown["group"].get("rules"), None);
    assert_eq!(shown["group"].get("labels"), None);

    // Changed in place, it keeps its key, so a label citing it still does.
    let changed =
        json!({"$type": collection, "title": "Be very kind", "text": "Always.", "order": 5});
    let said = app
        .put(
            &h,
            &token,
            &members,
            &collection,
            &first_key,
            changed.clone(),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(said.body["uri"], first.body["uri"]);
    assert_eq!(rule(&first_key).unwrap(), changed);

    // Taken away, it is gone; taking it away again is no matter.
    let body = json!({"repo": did, "space": members, "collection": collection, "rkey": second_key});
    for _ in 0..2 {
        let said = app
            .call(&h, "com.atproto.space.deleteRecord", &token, Some(&body))
            .await;
        assert_eq!(said.status, 200, "{}", said.body);
    }
    assert_eq!(rule(&second_key), None);

    let at = async |rkey: &str, record: Json| {
        app.put(&h, &token, &members, &collection, rkey, record)
            .await
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
async fn an_admin_keeps_the_labeler_in_the_members_space() {
    let (h, did, [alice, bob, _]) = group().await;
    let members = members_space(&did);
    let (app, token) = console_in(&h, &did, &alice, &members).await;
    let collection = format!("{NS}.labeler");
    let mut labeler = kept(&h, &did, &members, "labeler", "self").unwrap();
    // Every group starts with one there, taking reports of every reason.
    assert_eq!(
        labeler["reasonTypes"],
        json!(group_host::group::REASONS.to_vec())
    );
    assert_eq!(kept(&h, &did, &meta_space(&did), "labeler", "self"), None);
    let defined = json!([{"identifier": "spoiler", "severity": "inform", "blurs": "content", "locales": [{"lang": "en", "name": "Spoiler", "description": "Gives it away."}]}]);
    labeler["policies"]["labelValueDefinitions"] = defined.clone();
    let said = app
        .put(&h, &token, &members, &collection, "self", labeler.clone())
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(view(&h, &did, Some(&bob)).await["group"]["labels"], defined);
    // Kept at self, and never taken away.
    let said = app
        .put(&h, &token, &members, &collection, "other", labeler)
        .await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
    let body = json!({"repo": did, "space": members, "collection": collection, "rkey": "self"});
    let said = app
        .call(&h, "com.atproto.space.deleteRecord", &token, Some(&body))
        .await;
    assert_eq!(said.error(), (400, "InvalidRequest"));
}

#[tokio::test]
async fn declares_the_group_while_anyone_may_read_its_meta() {
    let (h, did, [alice, _, _]) = group().await;
    // An app signed in as the group, Alice approving it.
    let app = App::loopback(&format!("atproto repo:{NS}.declaration"));
    let tokens = app.sign_in(&h, &alice, &did).await;
    let token = tokens["access_token"].as_str().unwrap().to_string();
    let collection = format!("{NS}.declaration");
    let declared = || h.host.store.record(&did, &collection, "self").unwrap();
    let put = async |rkey: &str, record: Json| {
        let body = json!({"repo": did, "collection": collection, "rkey": rkey, "record": record});
        app.call(&h, "com.atproto.repo.putRecord", &token, Some(&body))
            .await
    };
    // Empty: it says only that the account is a group.
    let declaration = json!({ "$type": collection });
    let first = declared().expect("a group anyone may read is declared");
    let first = drisl_json(&first.value);
    assert_eq!(first, declaration);
    // Taken away, the group is unlisted; put back, listed again.
    let body = json!({"repo": did, "collection": collection, "rkey": "self"});
    let said = app
        .call(&h, "com.atproto.repo.deleteRecord", &token, Some(&body))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert!(declared().is_none());
    let said = put("self", declaration.clone()).await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert!(declared().is_some());
    // At self alone.
    let said = put("other", declaration.clone()).await;
    assert_eq!(said.error(), (400, "InvalidRequest"));

    // Once only members read its meta, it is unlisted, and stays so.
    hide(&h, &alice, &did).await;
    assert!(declared().is_none());
    assert_eq!(
        put("self", declaration).await.error(),
        (400, "InvalidRequest")
    );
    assert!(declared().is_none());
}

/// A record's bytes as JSON.
fn drisl_json(bytes: &[u8]) -> Json {
    atproto::drisl::decode(bytes).unwrap().to_json()
}

/// Puts `did`'s meta space under a policy only its members read, as
/// `who`.
async fn hide(h: &common::Running, who: &Authority, did: &str) {
    let meta = meta_space(did);
    let policy =
        json!({"$type": format!("{NS}.policy"), "grants": [{"role": "member", "read": true}]});
    let body = json!({"space": meta, "policy": policy});
    let nsid = "com.atproto.space.updateSpace";
    let said = as_group(h, who, did, &meta, nsid, Some(&body)).await;
    assert_eq!(said.status, 200, "{}", said.body);
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
            format!(
                "{}={cookie}",
                h.host.cookie_name(group_host::console_login::COOKIE)
            ),
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
    let collection = format!("{NS}.about");
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

    // Unlisted, its about record is its readers' alone: a member's, not a
    // stranger's.
    hide(&h, &alice, &did).await;
    assert_eq!(image(&h, &did, &cid(&avatar), None).await.0, 404);
    assert_eq!(image(&h, &did, &cid(&avatar), Some(&carol)).await.0, 404);
    assert_eq!(image(&h, &did, &cid(&avatar), Some(&bob)).await.0, 200);

    // Held only in a space, it is no blob of the public repo's: neither
    // fetched nor listed by its sync, which asks no one who they are.
    let synced = async |cid: String| {
        let query = format!("did={did}&cid={cid}");
        h.get("com.atproto.sync.getBlob", &query).await
    };
    let (status, said) = synced(cid(&avatar)).await;
    assert_eq!(
        (status, said["error"].as_str()),
        (400, Some("BlobNotFound"))
    );
    let (_, listed) = h
        .get("com.atproto.sync.listBlobs", &format!("did={did}"))
        .await;
    assert_eq!(listed["cids"], json!([]));

    // In its public repo: anyone's, there too.
    let photo = uploaded(&h, &app, &token, b"ride".to_vec(), "image/jpeg").await;
    h.host.write(&did, &[holding("ride", &photo)]).unwrap();
    assert_eq!(image(&h, &did, &cid(&photo), None).await.0, 200);
    assert_eq!(synced(cid(&photo)).await.0, 200);

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
