//! A group's website: published whole by those who configure the group,
//! and served as it is, sandboxed, at the group's address.

mod common;

use common::groups::*;
use common::members::{granting, keep};
use common::oauth::App;
use common::spaces::*;
use common::{Running, start, start_naming_itself};
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
    let membership = json!({"$type": format!("{NS}.membership"), "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"});
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
    // Its spaces each a type, named once.
    for spaces in [
        json!([{"space": "a.b.c"}, {"space": "a.b.c"}]),
        json!([{"space": ""}]),
    ] {
        let mut record = site("index.html");
        record["spaces"] = spaces;
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
    let member = granting(&h, &did, "member", &[&format!("repo:{SITE}")], &[]);
    keep(&h, &did, "role", "member", member).await;
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

/// The page around a live site's pages, as the host writes it: its head,
/// and the settings its script reads.
fn shell_of(page: &Page) -> (&str, Json) {
    let head = page.body.split("</head>").next().unwrap();
    let config = page.body.split("id=\"os-config\">").nth(1).unwrap();
    let config = config.split("</script>").next().unwrap();
    (head, serde_json::from_str(config).unwrap())
}

#[tokio::test]
async fn a_site_that_reads_is_served_in_the_page_that_holds_its_visitors_session() {
    let built = tempfile::tempdir().unwrap();
    std::fs::write(built.path().join("shell.js"), "// the shell").unwrap();
    let dir = built.path().to_str().unwrap();
    let vars = [
        ("SITE_SHELL_DIR", dir),
        ("SITE_HANDLE_RESOLVER", "https://pds.test"),
        ("SITE_PLC_URL", "https://plc.test"),
    ];
    let h = start(&vars, false).await;
    let (did, [alice, ..]) = group(&h).await;
    let front = "<html><head><title> Riders &amp; friends </title>\
        <meta property=\"og:image\" content=\"/card.png\"></head><body>live</body></html>";
    let files = json!([
        {"path": "index.html", "text": front},
        {"path": "rides/index.html", "text": "<h1>Rides</h1>"},
        {"path": "card.png", "bytes": {"$bytes": "iVBORw0KGgo"}},
        {"path": "404.html", "text": "<title>Lost</title>"},
    ]);
    let reads =
        json!([{"method": format!("{NS}.getGroup")}, {"method": format!("{NS}.listMembers")}]);
    let body = json!({"group": did, "files": files, "notFound": "404.html", "reads": reads});
    let made = call(&h, &alice, "putSite", body.clone()).await;
    assert_eq!(made.0, 200, "{}", made.1);

    // A page is the host's, around the site's own in a sandboxed frame.
    let shell = page(&h, "/").await;
    assert_eq!(shell.status, 200);
    assert!(
        shell
            .header("content-security-policy")
            .contains("frame-ancestors 'none'")
    );
    assert_eq!(shell.header("cache-control"), "no-cache");
    assert!(
        shell.body.contains("data-src=\"/_site/\""),
        "{}",
        shell.body
    );
    let (head, config) = shell_of(&shell);
    assert!(
        head.contains("<title>Riders &amp; friends</title>"),
        "{head}"
    );
    assert!(head.contains("<meta property=\"og:image\" content=\"/card.png\">"));
    assert_eq!(config["group"], did);
    assert_eq!(
        config["reads"],
        json!([format!("{NS}.getGroup"), format!("{NS}.listMembers")])
    );
    assert_eq!(config["handleResolver"], "https://pds.test");
    assert_eq!(config["plc"], "https://plc.test");
    // Its frame sized before the page's script runs.
    assert!(
        shell
            .body
            .contains("#os-page{display:block;width:100%;height:100%;border:0}")
    );
    // A page the site has no file for is missing there too, its preview
    // the notFound page's; a site with neither, the group's handle.
    let lost = page(&h, "/rides/42").await;
    assert_eq!(lost.status, 404);
    assert!(shell_of(&lost).0.contains("<title>Lost</title>"));
    assert!(lost.body.contains("data-src=\"/_site/rides/42\""));
    assert_eq!(page(&h, "/rides").await.status, 200);

    // The site's own files, sandboxed, under /_site/; its folders too.
    let framed = page(&h, "/_site/").await;
    assert_eq!(
        (framed.status, framed.header("content-security-policy")),
        (200, SANDBOX)
    );
    assert!(framed.body.contains("live"));
    let folder = page(&h, "/_site/rides").await;
    assert_eq!(
        (folder.status, folder.header("location")),
        (301, "/_site/rides/")
    );
    // A file that is no page, at its own path, as the frame's links ask.
    let image = page(&h, "/card.png").await;
    assert_eq!(
        (image.status, image.header("content-type")),
        (200, "image/png")
    );

    // The scripts: the kit a page loads, and the host's own page's.
    let kit = page(&h, "/_os/kit.js").await;
    assert_eq!(kit.header("content-type"), "text/javascript; charset=utf-8");
    assert!(kit.body.contains("customElements.define('os-sign-in'"));
    assert_eq!(page(&h, "/_os/shell.js").await.body, "// the shell");

    // Its OAuth client: at the address, asking only for what it reads.
    let client = page(&h, "/client-metadata.json").await;
    let doc: Json = serde_json::from_str(&client.body).unwrap();
    let origin = format!("https://{RIDERS}");
    assert_eq!(doc["client_id"], format!("{origin}/client-metadata.json"));
    assert_eq!(
        doc["redirect_uris"],
        json!([format!("{origin}/_os/callback")])
    );
    let scope = doc["scope"]
        .as_str()
        .unwrap()
        .split(' ')
        .collect::<Vec<_>>();
    let aud = "aud=did%3Aweb%3Ahost.test%23opensocial";
    let rpc = |m: &str| format!("rpc:{NS}.{m}?{aud}");
    let wanted = [
        "atproto".into(),
        rpc("getGroup"),
        rpc("getViewer"),
        rpc("listMembers"),
    ];
    assert_eq!(scope, wanted);
    // Behind a proxy, it is named for the name the browser asked.
    let proxied = [("x-forwarded-host", "riders.example:8443")];
    let client = ask(&h, "GET", RIDERS, "/client-metadata.json", &proxied).await;
    let doc: Json = serde_json::from_str(&client.body).unwrap();
    let callback = json!(["https://riders.example:8443/_os/callback"]);
    assert_eq!(doc["redirect_uris"], callback);

    // The host's methods answer there, for the page around the site.
    let health = page(&h, "/xrpc/_health").await;
    assert_eq!(
        (health.status, health.header("content-security-policy")),
        (200, "")
    );

    // Published without reads, it is its files alone again.
    let mut plain = body;
    plain.as_object_mut().unwrap().remove("reads");
    assert_eq!(call(&h, &alice, "putSite", plain).await.0, 200);
    let front = page(&h, "/").await;
    assert_eq!(
        (front.status, front.header("content-security-policy")),
        (200, SANDBOX)
    );
    assert!(front.body.contains("live"));
}

#[tokio::test]
async fn a_site_reads_only_what_the_host_lets_a_page_read() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let files = json!([{"path": "index.html", "text": "<p>hi</p>"}]);
    let reads = json!([{"method": format!("{NS}.createGroup")}]);
    let said = call(
        &h,
        &alice,
        "putSite",
        json!({"group": did, "files": files, "reads": reads}),
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidSite"));
    let message = said.1["message"].as_str().unwrap();
    assert!(
        message.contains("createGroup is none of the methods"),
        "{message}"
    );

    // Without a built page to hold a session, a page of the site still
    // loads, and says what it lacks.
    let reads = json!([{"method": format!("{NS}.getGroup")}]);
    let said = call(
        &h,
        &alice,
        "putSite",
        json!({"group": did, "files": files, "reads": reads}),
    )
    .await;
    assert_eq!(said.0, 200, "{}", said.1);
    let shell = page(&h, "/_os/shell.js").await;
    assert_eq!(
        (shell.status, shell.body.as_str()),
        (404, "no page to run a site in here")
    );
    // A site with no page anywhere for a path, its preview the handle's.
    let lost = page(&h, "/nowhere").await;
    assert_eq!(lost.status, 404);
    assert!(
        shell_of(&lost)
            .0
            .contains(&format!("<title>{RIDERS}</title>"))
    );
}

#[tokio::test]
async fn a_site_acts_as_its_visitor_only_as_the_host_lets_a_page() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let files = json!([{"path": "index.html", "text": "<p>the board</p>"}]);

    // A procedure that is no act a page may ask is refused, such as one
    // that would make the visitor's group for them.
    let acts = json!([{"method": format!("{NS}.createGroup")}]);
    let said = call(
        &h,
        &alice,
        "putSite",
        json!({"group": did, "files": files, "acts": acts}),
    )
    .await;
    assert_eq!(error(&said), (400, "InvalidSite"));
    let message = said.1["message"].as_str().unwrap();
    assert!(
        message.contains("createGroup is none of the acts"),
        "{message}"
    );

    // Reporting, joining, cancelling a request and leaving, though they
    // read and write nothing, have its pages served in the page that
    // holds a session, which is told what they may do.
    let named = [
        "createReport",
        "requestJoin",
        "cancelJoinRequest",
        "leaveGroup",
    ];
    let methods: Vec<String> = named.iter().map(|m| format!("{NS}.{m}")).collect();
    let acts: Json = methods.iter().map(|m| json!({"method": m})).collect();
    let said = call(
        &h,
        &alice,
        "putSite",
        json!({"group": did, "files": files, "acts": acts}),
    )
    .await;
    assert_eq!(said.0, 200, "{}", said.1);
    let shell = page(&h, "/").await;
    let (_, config) = shell_of(&shell);
    assert_eq!(config["acts"], json!(methods));

    // Its OAuth client asks the host for each, as for a read.
    let client = page(&h, "/client-metadata.json").await;
    let doc: Json = serde_json::from_str(&client.body).unwrap();
    let scope: Vec<&str> = doc["scope"].as_str().unwrap().split(' ').collect();
    let aud = "aud=did%3Aweb%3Ahost.test%23opensocial";
    let wanted = [
        "atproto".to_string(),
        format!("rpc:{NS}.cancelJoinRequest?{aud}"),
        format!("rpc:{NS}.createReport?{aud}"),
        format!("rpc:{NS}.getViewer?{aud}"),
        format!("rpc:{NS}.leaveGroup?{aud}"),
        format!("rpc:{NS}.requestJoin?{aud}"),
    ];
    assert_eq!(scope, wanted);

    // The record says so, for any app that reads it.
    let kept = h.host.store.record(&did, SITE, "self").unwrap().unwrap();
    let site = atproto::drisl::decode(&kept.value).unwrap().to_json();
    assert_eq!(site["acts"], acts);
}

#[tokio::test]
async fn a_site_reads_the_groups_spaces_whole_as_its_visitor() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let files = json!([{"path": "index.html", "text": "<p>the board</p>"}]);

    // A space named twice, whole or by its key, is refused; one naming
    // none is no NSID, refused by the lexicon before the host looks.
    let board = json!({"space": "com.example.board"});
    let pins = json!({"space": "com.example.board", "key": "pins"});
    let refused = [
        (json!([board, board]), "InvalidSite"),
        (json!([pins, pins]), "InvalidSite"),
        (json!([{"space": ""}]), "InvalidRequest"),
    ];
    for (spaces, wanted) in refused {
        let said = call(
            &h,
            &alice,
            "putSite",
            json!({"group": did, "files": files, "spaces": spaces}),
        )
        .await;
        assert_eq!(error(&said), (400, wanted), "{spaces}");
    }

    // Reading spaces alone, its pages are served in the page that holds a
    // session, which is told which spaces they read. A type named whole and
    // one space of it by key is redundant, but no harm.
    let members = format!("{NS}.members");
    let spaces = json!([board, pins, {"space": members}]);
    let said = call(
        &h,
        &alice,
        "putSite",
        json!({"group": did, "files": files, "spaces": spaces}),
    )
    .await;
    assert_eq!(said.0, 200, "{}", said.1);
    let shell = page(&h, "/").await;
    assert!(
        shell.body.contains("data-src=\"/_site/\""),
        "{}",
        shell.body
    );
    let (_, config) = shell_of(&shell);
    assert_eq!(config["spaces"], spaces);

    // Its OAuth client asks to read each, with the group as authority, and
    // the one space alone where it names one by key.
    let client = page(&h, "/client-metadata.json").await;
    let doc: Json = serde_json::from_str(&client.body).unwrap();
    let scope: Vec<&str> = doc["scope"].as_str().unwrap().split(' ').collect();
    let wanted = [
        "atproto".to_string(),
        format!("rpc:{NS}.getViewer?aud=did%3Aweb%3Ahost.test%23opensocial"),
        format!("space:com.example.board?authority={did}&action=read"),
        format!("space:com.example.board?authority={did}&skey=pins&action=read"),
        format!("space:{members}?authority={did}&action=read"),
    ];
    assert_eq!(scope, wanted);

    // The record says so, for any app that reads it.
    let kept = h.host.store.record(&did, SITE, "self").unwrap().unwrap();
    let site = atproto::drisl::decode(&kept.value).unwrap().to_json();
    assert_eq!(site["spaces"], spaces);
}

#[tokio::test]
async fn a_site_that_writes_asks_its_visitor_for_those_writes_alone() {
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    let files = json!([{"path": "index.html", "text": "<p>the board</p>"}]);
    let board = "com.example.board";
    let writes = json!([
        {"space": board, "collection": "com.example.note"},
        {"space": board, "key": "pins", "collection": "com.example.note"},
        {"space": format!("{NS}.members"), "collection": format!("{NS}.acceptance")},
    ]);
    let said = call(
        &h,
        &alice,
        "putSite",
        json!({"group": did, "files": files, "writes": writes}),
    )
    .await;
    assert_eq!(said.0, 200, "{}", said.1);

    // Writing, though it reads nothing, its pages are served in the page
    // that holds a session, which is told what they may write.
    let shell = page(&h, "/").await;
    assert!(
        shell.body.contains("data-src=\"/_site/\""),
        "{}",
        shell.body
    );
    let (_, config) = shell_of(&shell);
    assert_eq!(config["writes"], writes);
    assert_eq!(config["reads"], json!([]));

    // Its OAuth client asks the visitor's PDS for each write, in this
    // group's spaces of that type and in that collection alone, and to
    // upload pictures for those records to point to, beside asking the
    // host who they are.
    let client = page(&h, "/client-metadata.json").await;
    let doc: Json = serde_json::from_str(&client.body).unwrap();
    let scope: Vec<&str> = doc["scope"].as_str().unwrap().split(' ').collect();
    let space = |kind: &str, skey: &str, collection: &str| {
        format!(
            "space:{kind}?authority={did}{skey}&collection={collection}&action=create&action=update&action=delete"
        )
    };
    let wanted = [
        "atproto".to_string(),
        format!("rpc:{NS}.getViewer?aud=did%3Aweb%3Ahost.test%23opensocial"),
        space(board, "", "com.example.note"),
        space(board, "&skey=pins", "com.example.note"),
        space(&format!("{NS}.members"), "", &format!("{NS}.acceptance")),
        "blob:image/*".to_string(),
    ];
    assert_eq!(scope, wanted);

    // The record says so, for any app that reads it.
    let kept = h.host.store.record(&did, SITE, "self").unwrap().unwrap();
    let site = atproto::drisl::decode(&kept.value).unwrap().to_json();
    assert_eq!(site["writes"], writes);
}

#[test]
fn a_page_previews_as_its_own_head_says_and_never_as_markup() {
    let preview = |html: &str| group_host::site_head::preview(html, "riders.test");
    // The title, trimmed; none, or an empty one, is the fallback's.
    assert_eq!(preview("<TITLE> Rides </TITLE>"), "<title>Rides</title>");
    assert_eq!(preview("<p>no head</p>"), "<title>riders.test</title>");
    assert_eq!(preview("<title> </title>"), "<title>riders.test</title>");
    assert_eq!(preview("<title>never closed"), "<title>riders.test</title>");

    // The tags a preview is read from, however their attributes are
    // written; nothing else, and nothing after the head.
    let head = r##"<title>R</title>
        <meta name="description" content='Rides, "all paces"'>
        <META PROPERTY=og:title CONTENT=Riders />
        <meta name="twitter:card" content="summary" defer>
        <meta name=theme-color content="#0a0">
        <meta name="robots" content="noindex">
        <meta property="fb:app_id" content="1">
        <meta http-equiv="refresh" content="0;url=https://elsewhere.test">
        <meta charset="utf-8">
        <metadata name="description" content="no tag">
        <meta name="description" content="unclosed>
        </head><body><meta name="description" content="in the body">"##;
    assert_eq!(
        preview(head),
        "<title>R</title>\
         <meta name=\"description\" content=\"Rides, &quot;all paces&quot;\">\
         <meta property=\"og:title\" content=\"Riders\">\
         <meta name=\"twitter:card\" content=\"summary\">\
         <meta name=\"theme-color\" content=\"#0a0\">"
    );

    // What a page wrote can't end the host's tags or open one of its own;
    // an entity it wrote stays as written.
    let sly = "<title>1 < 2 &amp; \"q\" > 0</title>\
        <meta property='og:x\"><script>' content=\"a\">";
    assert_eq!(
        preview(sly),
        "<title>1 &lt; 2 &amp; &quot;q&quot; &gt; 0</title>\
         <meta property=\"og:x&quot;&gt;&lt;script&gt;\" content=\"a\">"
    );

    // Only so much of a page is read, at a character's edge, and only so
    // many tags taken from it.
    let long = format!("a{}<title>late</title>", "é".repeat(40_000));
    assert_eq!(preview(&long), "<title>riders.test</title>");
    let many = "<meta name=description content=x>".repeat(40);
    assert_eq!(preview(&many).matches("<meta").count(), 32);
}

/// Riders' handle on a domain of its own, as updateGroupHandle gives it.
const OWN: &str = "riders.example";

/// Whether a proxy in front may get a certificate for `domain`.
async fn certified(h: &Running, domain: &str) -> u16 {
    let asked = reqwest::get(format!("{}/tls/ask?domain={domain}", h.url)).await;
    asked.unwrap().status().as_u16()
}

#[tokio::test]
async fn a_site_moves_to_the_groups_own_domain_once_it_reaches_the_host() {
    // A host whose groups' domains reach it, as the proxy in front would.
    let h = start_naming_itself("SITE_CHECK_AT", &[]).await;
    let (did, [alice, bob, ..]) = group(&h).await;
    let site = json!({"group": did, "files": [{"path": "index.html", "text": "hi"}]});
    assert_eq!(call(&h, &alice, "putSite", site.clone()).await.0, 200);
    let body = json!({"group": did});
    // Its handle its address, it has no domain of its own to move to.
    let said = call(&h, &alice, "putSiteDomain", body.clone()).await;
    assert_eq!(error(&said), (400, "NoOwnDomain"));
    assert_eq!(certified(&h, RIDERS).await, 404);

    h.host.store.set_handle(&did, OWN, &[]).unwrap();
    // Certificates for its domain and www. under it, before it is switched
    // on, so the host can check the domain reaches it; for no other.
    assert_eq!(certified(&h, OWN).await, 200);
    assert_eq!(certified(&h, "WWW.riders.example").await, 200);
    assert_eq!(certified(&h, "elsewhere.example").await, 404);
    // Until it is, the domain serves nothing of the site.
    let there = ask(&h, "GET", OWN, "/", &[]).await;
    assert_eq!(there.header("content-security-policy"), "");
    let said = call(&h, &bob, "putSiteDomain", body.clone()).await;
    assert_eq!(error(&said), (403, "Forbidden"));

    let said = call(&h, &alice, "putSiteDomain", body.clone()).await;
    assert_eq!(said, (200, json!({"domain": OWN})));
    // The domain serves it, keeping the host's own paths; the address and
    // www. send visitors there, path and query kept.
    let there = ask(&h, "GET", OWN, "/", &[]).await;
    assert_eq!((there.status, there.body.as_str()), (200, "hi"));
    let from = page(&h, "/rides/?when=sat").await;
    let to = "https://riders.example/rides/?when=sat";
    assert_eq!((from.status, from.header("location")), (301, to));
    assert_eq!(from.header("content-security-policy"), SANDBOX);
    let www = ask(&h, "GET", "www.riders.example", "/a", &[]).await;
    assert_eq!(
        (www.status, www.header("location")),
        (301, "https://riders.example/a")
    );
    let named = ask(&h, "GET", OWN, "/.well-known/atproto-did", &[]).await;
    assert_eq!(named.body, did);
    // The group and a publish say where it is.
    let lxm = format!("{NS}.getGroup");
    let (_, shown) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(shown["group"]["siteDomain"], OWN);
    let made = call(&h, &alice, "putSite", site).await;
    assert_eq!(made.1["url"], "https://riders.example/");

    // Switched off, the site is at its address alone again.
    assert_eq!(
        call(&h, &alice, "deleteSiteDomain", body.clone()).await,
        (200, json!({}))
    );
    assert_eq!(page(&h, "/").await.body, "hi");
    let there = ask(&h, "GET", OWN, "/", &[]).await;
    assert_eq!(there.header("content-security-policy"), "");
    let (_, shown) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(shown["group"].get("siteDomain"), None);

    // On again, a new handle ends it: the new domain was never checked.
    call(&h, &alice, "putSiteDomain", body).await;
    let lxm = format!("{NS}.updateGroupHandle");
    let back = json!({"group": did, "handle": RIDERS});
    assert_eq!(
        post(&h, &lxm, &back, &[auth(&h, &alice, &lxm)]).await.0,
        200
    );
    assert_eq!(page(&h, "/").await.body, "hi");
    h.host.store.set_handle(&did, OWN, &[]).unwrap();
    assert_eq!(page(&h, "/").await.body, "hi");
}

#[tokio::test]
async fn a_site_stays_at_its_address_while_its_domain_reaches_no_host() {
    // Asked as itself, over https, a domain under .example never answers.
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    h.host.store.set_handle(&did, OWN, &[]).unwrap();
    let said = call(&h, &alice, "putSiteDomain", json!({"group": did})).await;
    assert_eq!(error(&said), (400, "NotReaching"));
    let message = said.1["message"].as_str().unwrap();
    assert!(
        message.starts_with("riders.example doesn't reach this host yet"),
        "{message}"
    );
}
