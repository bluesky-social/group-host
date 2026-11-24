//! A group at its address, and at its own domain once a steward switches
//! that on and the domain reaches the host.

mod common;

use common::groups::*;
use common::members::keep;
use common::spaces::*;
use common::{Running, start, start_naming_itself};
use serde_json::{Value as Json, json};

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{INTERNAL}.{method}");
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

/// What a name answers: status, where it sends the visitor, and body.
struct Page {
    status: u16,
    location: String,
    body: String,
}

/// `method path` asked of the host as `name`.
async fn ask(h: &Running, method: &str, name: &str, path: &str) -> Page {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
    let res = client
        .request(method, format!("{}{path}", h.url))
        .header("host", name)
        .send()
        .await
        .unwrap();
    let location = res.headers().get("location");
    Page {
        status: res.status().as_u16(),
        location: location.map_or("", |v| v.to_str().unwrap()).to_string(),
        body: res.text().await.unwrap(),
    }
}

async fn page(h: &Running, path: &str) -> Page {
    ask(h, "GET", RIDERS, path).await
}

#[tokio::test]
async fn an_address_sends_its_visitor_to_the_groups_page_and_keeps_the_hosts_paths() {
    let h = start(&[("CONSOLE_URL", "https://console.test")], false).await;
    let (did, [alice, ..]) = group(&h).await;
    // Its page in the console, wherever the console is.
    let front = page(&h, "/").await;
    assert_eq!(
        (front.status, front.location.as_str()),
        (302, "https://console.test/g/riders.groups.test")
    );
    assert_eq!(ask(&h, "HEAD", RIDERS, "/").await.status, 302);
    let none = page(&h, "/about").await;
    assert_eq!((none.status, none.body.as_str()), (404, "nothing here"));
    assert_eq!(ask(&h, "POST", RIDERS, "/").await.status, 404);
    // The host's own paths there.
    assert_eq!(page(&h, "/.well-known/atproto-did").await.body, did);
    assert_eq!(page(&h, "/xrpc/_health").await.status, 200);
    // Another name passes on as to the host.
    assert_eq!(
        ask(&h, "GET", "elsewhere.test", "/xrpc/_health")
            .await
            .status,
        200
    );
    assert_eq!(
        ask(&h, "GET", "www.elsewhere.test", "/xrpc/_health")
            .await
            .status,
        200
    );

    // A group switched off has nothing there.
    h.host.deactivate_account(&did, &alice.did, &[]).unwrap();
    assert_eq!(page(&h, "/").await.status, 404);
}

/// Riders' handle on a domain of its own, as updateGroupHandle gives it.
const OWN: &str = "riders.example";

/// Whether a proxy in front may get a certificate for `domain`.
async fn certified(h: &Running, domain: &str) -> u16 {
    let asked = reqwest::get(format!("{}/tls/ask?domain={domain}", h.url)).await;
    asked.unwrap().status().as_u16()
}

#[tokio::test]
async fn a_group_moves_to_its_own_domain_once_it_reaches_the_host() {
    // A host whose groups' domains reach it, as the proxy in front would.
    let console = [("CONSOLE_URL", "https://console.test")];
    let h = start_naming_itself("DOMAIN_CHECK_AT", &console).await;
    let (did, [alice, bob, ..]) = group(&h).await;
    let body = json!({"group": did});
    // Its handle its address, it has no domain of its own to move to.
    let said = call(&h, &alice, "putGroupDomain", body.clone()).await;
    assert_eq!(error(&said), (400, "NoOwnDomain"));
    assert_eq!(certified(&h, RIDERS).await, 404);

    h.host.store.set_handle(&did, OWN, &[]).unwrap();
    // Certificates for its domain and www. under it, before it is switched
    // on, so the host can check the domain reaches it; for no other.
    assert_eq!(certified(&h, OWN).await, 200);
    assert_eq!(certified(&h, "WWW.riders.example").await, 200);
    assert_eq!(certified(&h, "elsewhere.example").await, 404);
    // Until it is, the domain is no group's here.
    assert_eq!(ask(&h, "GET", OWN, "/").await.status, 404);
    let said = call(&h, &bob, "putGroupDomain", body.clone()).await;
    assert_eq!(error(&said), (403, "Forbidden"));

    let said = call(&h, &alice, "putGroupDomain", body.clone()).await;
    assert_eq!(said, (200, json!({"domain": OWN})));
    // The domain is the group's, keeping the host's own paths; the address
    // and www. send visitors there, path and query kept.
    let there = ask(&h, "GET", OWN, "/").await;
    assert_eq!(
        (there.status, there.location.as_str()),
        (302, "https://console.test/g/riders.example")
    );
    let from = page(&h, "/rides/?when=sat").await;
    let to = "https://riders.example/rides/?when=sat";
    assert_eq!((from.status, from.location.as_str()), (301, to));
    let www = ask(&h, "GET", "www.riders.example", "/a").await;
    assert_eq!(
        (www.status, www.location.as_str()),
        (301, "https://riders.example/a")
    );
    let named = ask(&h, "GET", OWN, "/.well-known/atproto-did").await;
    assert_eq!(named.body, did);
    // The group says where it is.
    let lxm = format!("{INTERNAL}.getGroup");
    let (_, shown) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(shown["group"]["domain"], OWN);

    // Switched off, it is at its address alone again.
    assert_eq!(
        call(&h, &alice, "deleteGroupDomain", body.clone()).await,
        (200, json!({}))
    );
    assert_eq!(page(&h, "/").await.status, 302);
    assert_eq!(ask(&h, "GET", OWN, "/").await.status, 404);
    let (_, shown) = get(&h, &lxm, &format!("group={did}"), &[auth(&h, &alice, &lxm)]).await;
    assert_eq!(shown["group"].get("domain"), None);

    // On again, a new handle ends it: the new domain was never checked.
    call(&h, &alice, "putGroupDomain", body).await;
    assert_eq!(page(&h, "/").await.status, 301);
    let lxm = format!("{INTERNAL}.updateGroupHandle");
    let back = json!({"group": did, "handle": RIDERS});
    assert_eq!(
        post(&h, &lxm, &back, &[auth(&h, &alice, &lxm)]).await.0,
        200
    );
    assert_eq!(page(&h, "/").await.status, 302);
    h.host.store.set_handle(&did, OWN, &[]).unwrap();
    assert_eq!(page(&h, "/").await.status, 302);
}

#[tokio::test]
async fn a_group_stays_at_its_address_while_its_domain_reaches_no_host() {
    // Asked as itself, over https, a domain under .example never answers.
    let h = start(&[], false).await;
    let (did, [alice, ..]) = group(&h).await;
    h.host.store.set_handle(&did, OWN, &[]).unwrap();
    let said = call(&h, &alice, "putGroupDomain", json!({"group": did})).await;
    assert_eq!(error(&said), (400, "NotReaching"));
    let message = said.1["message"].as_str().unwrap();
    assert!(
        message.starts_with("riders.example doesn't reach this host yet"),
        "{message}"
    );
}
