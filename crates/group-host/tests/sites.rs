//! A group's website: published whole by those who configure the group,
//! and served as it is, sandboxed, at the group's address.

mod common;

use common::groups::*;
use common::members::{granting, keep};
use common::oauth::App;
use common::spaces::*;
use common::{Running, start};
use group_host::sites::{self, SANDBOX, SITE};
use serde_json::{Value as Json, json};

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

fn error(said: &(u16, Json)) -> (u16, &str) {
    (said.0, said.1["error"].as_str().unwrap_or_default())
}

/// A group Alice started, and Bob a plain member of it.
async fn group(h: &Running) -> (String, [Authority; 3]) {
    let [alice, bob, carol] = people(h).await;
    let did = start_group(
        h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let membership = json!({"$type": format!("{NS}.membership"), "member": bob.did, "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"});
    keep(h, &did, "membership", &bob.did, membership).await;
    (did, [alice, bob, carol])
}

/// A small site: a front page, a page in a folder, a style sheet, and an
/// image given as bytes.
fn files() -> Json {
    json!([
        {"path": "index.html", "text": "<h1>Riders</h1>"},
        {"path": "rides/index.html", "text": "<h1>Rides</h1>"},
        {"path": "why?/index.html", "text": "why"},
        {"path": "style.css", "text": "h1 { color: red }"},
        {"path": "a b.png", "bytes": {"$bytes": "iVBORw0KGgo"}},
        {"path": "data.bin", "text": "{}", "contentType": "application/json"},
        {"path": "404.html", "text": "lost"},
        {"path": "app.html", "text": "app"},
    ])
}

/// What a site answers: status, headers and body.
struct Page {
    status: u16,
    headers: reqwest::header::HeaderMap,
    body: String,
}

impl Page {
    fn header(&self, name: &str) -> &str {
        self.headers.get(name).map_or("", |v| v.to_str().unwrap())
    }
}

/// `method path` asked of the site at `name`, with `headers`.
async fn ask(h: &Running, method: &str, name: &str, path: &str, headers: &[(&str, &str)]) -> Page {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
    let mut req = client
        .request(method, format!("{}{path}", h.url))
        .header("host", name);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = req.send().await.unwrap();
    Page {
        status: res.status().as_u16(),
        headers: res.headers().clone(),
        body: res.text().await.unwrap(),
    }
}

async fn page(h: &Running, path: &str) -> Page {
    ask(h, "GET", RIDERS, path, &[]).await
}

#[tokio::test]
async fn a_site_is_published_and_served_sandboxed() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let body =
        json!({"group": did, "files": files(), "notFound": "404.html", "fallback": "app.html"});
    let made = call(&h, &alice, "putSite", body.clone()).await;
    assert_eq!(made.0, 200, "{}", made.1);
    assert_eq!(made.1["uri"], format!("at://{did}/{SITE}/self"));
    assert_eq!(made.1["url"], format!("https://{RIDERS}/"));
    assert_eq!(made.1["uploaded"], 8);
    let record = h.host.store.record(&did, SITE, "self").unwrap().unwrap();
    assert_eq!(made.1["cid"], record.cid.to_string());

    let front = page(&h, "/").await;
    assert_eq!(
        (front.status, front.body.as_str()),
        (200, "<h1>Riders</h1>")
    );
    assert_eq!(front.header("content-type"), "text/html; charset=utf-8");
    assert_eq!(front.header("content-security-policy"), SANDBOX);
    assert_eq!(front.header("access-control-allow-origin"), "*");
    assert_eq!(front.header("x-content-type-options"), "nosniff");
    assert_eq!(front.header("cache-control"), "public, max-age=60");

    // The same file, held already, is not sent again.
    let etag = front.header("etag").to_string();
    let held = ask(&h, "GET", RIDERS, "/", &[("if-none-match", &etag)]).await;
    assert_eq!((held.status, held.body.as_str()), (304, ""));
    let head = ask(&h, "HEAD", RIDERS, "/style.css", &[]).await;
    assert_eq!(
        (head.status, head.header("content-type")),
        (200, "text/css; charset=utf-8")
    );

    assert_eq!(page(&h, "/rides/").await.body, "<h1>Rides</h1>");
    let folder = page(&h, "/rides").await;
    assert_eq!((folder.status, folder.header("location")), (301, "/rides/"));
    assert_eq!(folder.header("content-security-policy"), SANDBOX);
    // A folder's name is written back as a path in its Location.
    let asked = page(&h, "/why%3F").await;
    assert_eq!((asked.status, asked.header("location")), (301, "/why%3F/"));
    assert_eq!(page(&h, "/why%3F/").await.body, "why");
    let image = page(&h, "/a%20b.png").await;
    assert_eq!(
        (image.status, image.header("content-type")),
        (200, "image/png")
    );
    assert_eq!(
        page(&h, "/data.bin").await.header("content-type"),
        "application/json"
    );
    assert_eq!(page(&h, "/%FF").await.status, 404);

    // A page no file has is the app's; anything else missing, notFound's.
    let routed = page(&h, "/rides/42").await;
    assert_eq!((routed.status, routed.body.as_str()), (200, "app"));
    let asked = ask(&h, "GET", RIDERS, "/x.json", &[("accept", "text/html")]).await;
    assert_eq!(asked.body, "app");
    let lost = page(&h, "/gone.png").await;
    assert_eq!((lost.status, lost.body.as_str()), (404, "lost"));

    let posted = ask(&h, "POST", RIDERS, "/", &[]).await;
    assert_eq!((posted.status, posted.header("allow")), (405, "GET, HEAD"));
    assert_eq!(posted.header("content-security-policy"), SANDBOX);

    // The host's own, at the handle and at its own name, are as they were.
    let did_at = ask(&h, "GET", RIDERS, "/.well-known/atproto-did", &[]).await;
    assert_eq!(did_at.body, did);
    let own = ask(&h, "GET", "host.test", "/xrpc/_health", &[]).await;
    assert_eq!(own.status, 200);
    assert_eq!(own.header("content-security-policy"), "");

    // Published again, the files it holds are used again.
    let again = call(&h, &alice, "putSite", body).await;
    assert_eq!((again.0, &again.1["uploaded"]), (200, &json!(0)));
}

#[tokio::test]
async fn a_site_without_its_pages_answers_plainly() {
    let h = start(&[("CONSOLE_URL", "https://console.test")], false).await;
    let (did, [alice, ..]) = group(&h).await;
    // Its page in the console, wherever the console is.
    let console = page(&h, "/").await;
    assert_eq!(console.status, 302);
    assert_eq!(
        console.header("location"),
        format!("https://console.test/g/{RIDERS}")
    );
    assert_eq!(page(&h, "/about").await.status, 404);

    let only = json!([{"path": "about.html", "text": "about"}]);
    let made = call(&h, &alice, "putSite", json!({"group": did, "files": only})).await;
    assert_eq!(made.0, 200, "{}", made.1);
    let none = page(&h, "/missing").await;
    assert_eq!((none.status, none.body.as_str()), (404, "no such page"));
    assert_eq!(page(&h, "/").await.status, 404);

    // A group switched off has no site.
    h.host.deactivate_account(&did, &alice.did, &[]).unwrap();
    let off = page(&h, "/about.html").await;
    assert_eq!((off.status, off.body.as_str()), (404, "no site here"));
}

#[tokio::test]
async fn only_who_configures_the_group_publishes() {
    let h = start(&[], false).await;
    let (did, [alice, bob, ..]) = group(&h).await;
    let one = json!([{"path": "index.html", "text": "hi"}]);
    let said = call(&h, &bob, "putSite", json!({"group": did, "files": one})).await;
    assert_eq!(error(&said), (403, "Forbidden"));
    let said = call(&h, &bob, "deleteSite", json!({"group": did})).await;
    assert_eq!(error(&said), (403, "Forbidden"));

    // Its lexicon refuses bytes that are not base64; the host, a site it
    // would not serve.
    let bad = [
        (
            json!([{"path": "../up", "text": "x"}]),
            json!({}),
            "InvalidSite",
        ),
        (
            json!([{"path": "a", "text": "x"}, {"path": "a", "text": "y"}]),
            json!({}),
            "InvalidSite",
        ),
        (
            json!([{"path": "a", "text": "x"}]),
            json!({"fallback": "b"}),
            "InvalidSite",
        ),
        (json!([{"path": "a"}]), json!({}), "InvalidSite"),
        (
            json!([{"path": "a", "text": "x", "bytes": {"$bytes": "eA"}}]),
            json!({}),
            "InvalidSite",
        ),
        (
            json!([{"path": "a", "bytes": {"$bytes": "!!"}}]),
            json!({}),
            "InvalidRequest",
        ),
    ];
    for (files, more, name) in bad {
        let mut body = json!({"group": did, "files": files});
        body.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        let said = call(&h, &alice, "putSite", body).await;
        assert_eq!(error(&said), (400, name), "{}", said.1);
    }
    // A site goes in one body, as large as one upload may be.
    let big = "x".repeat(2000);
    let small = start(&[("BLOB_UPLOAD_LIMIT", "1000")], false).await;
    let (did, [alice, ..]) = group(&small).await;
    let files = json!([{"path": "a", "text": big}]);
    let said = call(
        &small,
        &alice,
        "putSite",
        json!({"group": did, "files": files}),
    )
    .await;
    assert_eq!(error(&said), (413, "PayloadTooLarge"), "{}", said.1);
}

#[tokio::test]
async fn a_site_is_taken_down() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let one = json!([{"path": "index.html", "text": "hi"}]);
    let made = call(&h, &alice, "putSite", json!({"group": did, "files": one})).await;
    assert_eq!(made.0, 200, "{}", made.1);
    let gone = call(&h, &alice, "deleteSite", json!({"group": did})).await;
    assert_eq!(gone.0, 200, "{}", gone.1);
    assert!(h.host.store.record(&did, SITE, "self").unwrap().is_none());
    assert_eq!(page(&h, "/").await.status, 302);
    // Taking down a site that is not there changes nothing.
    let again = call(&h, &alice, "deleteSite", json!({"group": did})).await;
    assert_eq!(again.0, 200, "{}", again.1);
}

#[tokio::test]
async fn a_site_stays_at_the_address_when_the_handle_moves_to_a_domain_of_its_own() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let body = json!({"group": did, "files": [{"path": "index.html", "text": "hi"}]});
    let made = call(&h, &alice, "putSite", body.clone()).await;
    assert_eq!(made.0, 200, "{}", made.1);
    // The handle updateGroupHandle would give it.
    h.host.store.set_handle(&did, "riders.test", &[]).unwrap();

    let front = page(&h, "/").await;
    assert_eq!((front.status, front.body.as_str()), (200, "hi"));
    assert_eq!(front.header("content-security-policy"), SANDBOX);
    // The domain is the group's name, not where its site is.
    let there = ask(&h, "GET", "riders.test", "/", &[]).await;
    assert_ne!(there.body, "hi");
    assert_eq!(there.header("content-security-policy"), "");
    // And putSite says the site is at the address.
    let again = call(&h, &alice, "putSite", body).await;
    assert_eq!(again.1["url"], format!("https://{RIDERS}/"));
}

#[tokio::test]
async fn an_app_writes_the_site_as_the_group_only_if_its_member_configures_it() {
    let h = start(&[], false).await;
    let (did, [alice, bob, ..]) = group(&h).await;
    let app = App::loopback(&format!("atproto repo:{SITE} blob:*/*"));
    let tokens = app.sign_in(&h, &alice, &did).await;
    let token = tokens["access_token"].as_str().unwrap();
    let uploaded = app.upload(&h, token, b"hi".to_vec(), "text/html").await;
    assert_eq!(uploaded.status, 200, "{}", uploaded.body);
    let blob = uploaded.body["blob"].clone();
    let site = |path: &str| json!({"$type": SITE, "files": [{"path": path, "blob": blob}], "createdAt": "2026-11-05T00:00:00Z"});
    let put = |rkey: &str, record: Json| json!({"repo": did, "collection": SITE, "rkey": rkey, "record": record});
    let said = app
        .call(
            &h,
            "com.atproto.repo.putRecord",
            token,
            Some(&put("self", site("index.html"))),
        )
        .await;
    assert_eq!(said.status, 200, "{}", said.body);
    assert_eq!(page(&h, "/").await.body, "hi");
    let said = app
        .call(
            &h,
            "com.atproto.repo.putRecord",
            token,
            Some(&put("other", site("index.html"))),
        )
        .await;
    assert_eq!(said.error().0, 400);
    let said = app
        .call(
            &h,
            "com.atproto.repo.putRecord",
            token,
            Some(&put("self", site("/abs"))),
        )
        .await;
    assert_eq!(said.error(), (400, "InvalidSite"));
    // Each file names a blob.
    let unnamed = [
        json!({"path": "index.html"}),
        json!({"path": "index.html", "blob": {"ref": {"$link": "nope"}}}),
    ];
    for file in unnamed {
        let record = json!({"$type": SITE, "files": [file], "createdAt": "2026-11-05T00:00:00Z"});
        let said = app
            .call(
                &h,
                "com.atproto.repo.putRecord",
                token,
                Some(&put("self", record)),
            )
            .await;
        assert_eq!(said.error(), (400, "InvalidSite"));
    }
    assert_eq!(page(&h, "/").await.body, "hi");
    let gone = json!({"repo": did, "collection": SITE, "rkey": "self"});
    let said = app
        .call(&h, "com.atproto.repo.deleteRecord", token, Some(&gone))
        .await;
    assert_eq!(said.status, 200, "{}", said.body);

    // A member whose role writes the record, but who does not configure
    // the group, may not.
    let p = granting(&h, &did, "member", &[&format!("repo:{SITE}")], &[]);
    keep(&h, &did, "permissions", "self", p).await;
    let tokens = app.sign_in(&h, &bob, &did).await;
    let token = tokens["access_token"].as_str().unwrap();
    let said = app
        .call(
            &h,
            "com.atproto.repo.putRecord",
            token,
            Some(&put("self", site("index.html"))),
        )
        .await;
    assert_eq!(said.error(), (403, "Forbidden"));
}

#[tokio::test]
async fn a_site_copied_from_elsewhere_with_no_blob_serves_nothing_there() {
    let h = start(&[], false).await;
    let (did, _) = group(&h).await;
    // As a move or an import keeps it, unchecked.
    let files = json!([{"path": "index.html", "blob": "none"}]);
    let site = json!({"$type": SITE, "files": files, "createdAt": "2026-11-05T00:00:00Z"});
    let write = group_host::repo::Write::Create {
        collection: SITE.into(),
        rkey: "self".into(),
        value: site,
    };
    h.host.write(&did, &[write]).unwrap();
    let front = page(&h, "/").await;
    assert_eq!((front.status, front.body.as_str()), (404, "no such page"));
}

#[test]
fn a_path_a_file_may_be_at() {
    for path in ["index.html", "a/b.css", "a b/c"] {
        assert!(sites::is_path(path), "{path}");
    }
    for path in [
        "",
        "/a",
        "a/",
        "a//b",
        "./a",
        "a/../b",
        "a\\b",
        "a\nb",
        &"a".repeat(513),
    ] {
        assert!(!sites::is_path(path), "{path}");
    }
    assert_eq!(sites::decoded("a%20b%2Fc%zz%"), Some("a b/c%zz%".into()));
    assert_eq!(sites::decoded("%C3"), None);
    assert_eq!(sites::encoded("a b/c?d#e%f~g.h"), "a%20b/c%3Fd%23e%25f~g.h");
}

#[test]
fn a_file_is_served_as_its_extension_says() {
    let kinds = [
        ("a.HTML", "text/html; charset=utf-8"),
        ("a.htm", "text/html; charset=utf-8"),
        ("a.mjs", "text/javascript; charset=utf-8"),
        ("a.js", "text/javascript; charset=utf-8"),
        ("a.map", "application/json"),
        ("a.json", "application/json"),
        ("a.md", "text/plain; charset=utf-8"),
        ("a.txt", "text/plain; charset=utf-8"),
        ("a.xml", "application/xml"),
        ("a.svg", "image/svg+xml"),
        ("a.jpg", "image/jpeg"),
        ("a.jpeg", "image/jpeg"),
        ("a.gif", "image/gif"),
        ("a.webp", "image/webp"),
        ("a.avif", "image/avif"),
        ("a.ico", "image/x-icon"),
        ("a.woff", "font/woff"),
        ("a.woff2", "font/woff2"),
        ("a.ttf", "font/ttf"),
        ("a.otf", "font/otf"),
        ("a.pdf", "application/pdf"),
        ("a.wasm", "application/wasm"),
        ("a.mp3", "audio/mpeg"),
        ("a.mp4", "video/mp4"),
        ("a.webm", "video/webm"),
        ("x.d/README", "application/octet-stream"),
    ];
    for (path, kind) in kinds {
        assert_eq!(sites::content_type(path), kind, "{path}");
    }
}

#[tokio::test]
async fn a_site_is_larger_than_another_methods_input() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let page = "x".repeat(group_host::lexicons::INPUT_AT_MOST + 1);
    let files = json!([{"path": "index.html", "text": page}]);
    let made = call(&h, &alice, "putSite", json!({"group": did, "files": files})).await;
    assert_eq!(made.0, 200, "{}", made.1);
}
