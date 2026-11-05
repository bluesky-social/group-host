//! MCP: a client, signed in at the host as MCP's authorization
//! spec has it, acting as the member who connected it, in the one group
//! they chose, with what their roles allow and nothing more.

mod common;

use atproto::jose;
use axum::Router;
use axum::routing::get as route;
use common::groups::*;
use common::spaces::*;
use common::{Running, start, start_reachable};
use group_host::group::members_space;
use group_host::repo::Write;
use serde_json::{Value as Json, json};
use std::collections::HashMap;

const BACK: &str = "http://127.0.0.1:4321/callback";

/// A browser that does not follow redirects, to read where they go.
fn browser() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn location(res: &reqwest::Response) -> String {
    res.headers()["location"].to_str().unwrap().to_string()
}

fn query(url: &str) -> HashMap<String, String> {
    reqwest::Url::parse(url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

async fn make_member(h: &Running, group: &str, who: &str) {
    let collection = format!("{NS}.membership");
    let value = json!({"$type": collection, "member": who, "roles": ["member"], "createdAt": "2026-11-03T00:00:00Z"});
    let write = Write::Create {
        collection,
        rkey: who.to_string(),
        value,
    };
    h.host
        .space_write(group, &members_space(group), &[write])
        .await
        .unwrap();
}

/// An MCP client, registered: its client ID.
async fn register(h: &Running) -> String {
    let res = h
        .http
        .post(format!("{}/mcp/register", h.url))
        .json(&json!({"client_name": "Claude", "redirect_uris": [BACK]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let said: Json = res.json().await.unwrap();
    assert_eq!(said["token_endpoint_auth_method"], "none");
    said["client_id"].as_str().unwrap().to_string()
}

/// A PKCE verifier and its challenge.
fn pkce() -> (String, String) {
    let verifier = jose::random(32);
    let challenge = jose::sha256(&verifier);
    (verifier, challenge)
}

/// The MCP client's authorization request, with `extra` changes: where
/// the browser is sent.
async fn authorize(
    h: &Running,
    client: &str,
    challenge: &str,
    extra: &[(&str, &str)],
) -> reqwest::Response {
    let mut q: HashMap<&str, &str> = HashMap::from([
        ("response_type", "code"),
        ("client_id", client),
        ("redirect_uri", BACK),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", "s1"),
        ("resource", "https://host.test/mcp"),
    ]);
    for (k, v) in extra {
        match *v {
            "" => q.remove(k),
            _ => q.insert(k, v),
        };
    }
    browser()
        .get(format!("{}/mcp/authorize", h.url))
        .query(&q)
        .send()
        .await
        .unwrap()
}

/// The request the console's approval page is sent with.
async fn requested(h: &Running, client: &str, challenge: &str) -> String {
    let res = authorize(h, client, challenge, &[]).await;
    assert_eq!(res.status(), 303);
    let to = location(&res);
    assert!(to.starts_with("https://host.test/authorize?"), "{to}");
    query(&to)["request_uri"].clone()
}

async fn call(h: &Running, who: &Authority, method: &str, body: Json) -> (u16, Json) {
    let lxm = format!("{NS}.{method}");
    post(h, &lxm, &body, &[auth(h, who, &lxm)]).await
}

async fn token(h: &Running, form: &[(&str, &str)]) -> (u16, Json) {
    let res = h
        .http
        .post(format!("{}/mcp/token", h.url))
        .form(form)
        .send()
        .await
        .unwrap();
    (res.status().as_u16(), res.json().await.unwrap())
}

/// Connects an MCP client as `who`, in `group`: its client ID and tokens.
async fn connect(h: &Running, who: &Authority, group: &str) -> (String, Json) {
    let client = register(h).await;
    let (verifier, challenge) = pkce();
    let uri = requested(h, &client, &challenge).await;
    let (status, approved) = call(
        h,
        who,
        "approveAuthorization",
        json!({"requestUri": uri, "group": group}),
    )
    .await;
    assert_eq!(status, 200, "{approved}");
    let back = query(approved["redirectUrl"].as_str().unwrap());
    assert_eq!(back["state"], "s1");
    assert_eq!(back["iss"], "https://host.test/mcp");
    let (status, tokens) = token(
        h,
        &[
            ("grant_type", "authorization_code"),
            ("code", &back["code"]),
            ("redirect_uri", BACK),
            ("code_verifier", &verifier),
            ("client_id", &client),
        ],
    )
    .await;
    assert_eq!(status, 200, "{tokens}");
    (client, tokens)
}

/// What the MCP server answers `message`, sent with `token`.
async fn mcp(h: &Running, token: Option<&str>, message: Json) -> reqwest::Response {
    let mut req = h.http.post(format!("{}/mcp", h.url)).json(&message);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    req.send().await.unwrap()
}

async fn tool(h: &Running, token: &str, name: &str, arguments: Json) -> Json {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}});
    let res = mcp(h, Some(token), message).await;
    assert_eq!(res.status(), 200);
    res.json::<Json>().await.unwrap()["result"].clone()
}

#[tokio::test]
async fn says_where_an_mcp_client_signs_in() {
    let h = start(&[], false).await;
    let (_, resource) = h
        .get_path("/.well-known/oauth-protected-resource/mcp")
        .await;
    assert_eq!(resource["resource"], "https://host.test/mcp");
    assert_eq!(
        resource["authorization_servers"],
        json!(["https://host.test/mcp"])
    );
    let (_, server) = h
        .get_path("/.well-known/oauth-authorization-server/mcp")
        .await;
    assert_eq!(server["issuer"], "https://host.test/mcp");
    assert_eq!(server["token_endpoint"], "https://host.test/mcp/token");
    assert_eq!(server["code_challenge_methods_supported"], json!(["S256"]));
    // Without a token, the challenge says where to find out.
    let res = mcp(&h, None, json!({})).await;
    assert_eq!(res.status(), 401);
    let said = res.headers()["www-authenticate"].to_str().unwrap();
    assert!(
        said.contains(
            r#"resource_metadata="https://host.test/.well-known/oauth-protected-resource/mcp""#
        ),
        "{said}"
    );
    let res = mcp(&h, Some("nothing"), json!({})).await;
    let said = res.headers()["www-authenticate"].to_str().unwrap();
    assert!(
        said.starts_with(r#"Bearer error="invalid_token""#),
        "{said}"
    );
}

#[tokio::test]
async fn acts_as_the_member_in_the_group_they_chose() {
    let h = start(&[], false).await;
    let [alice, bob, carol] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let client = register(&h).await;
    let (_, challenge) = pkce();
    let uri = requested(&h, &client, &challenge).await;

    // The approval page reads it as an MCP client's.
    let lxm = format!("{NS}.getAuthorization");
    let (status, view) = get(
        &h,
        &lxm,
        &format!("requestUri={uri}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["mcp"], true);
    assert_eq!(view["client"]["name"], "Claude");
    assert_eq!(view["groups"][0]["did"], group.as_str());
    assert_eq!(view["groups"][0]["granted"], json!([]));

    let (_, tokens) = connect(&h, &alice, &group).await;
    let access = tokens["access_token"].as_str().unwrap();
    assert_eq!(tokens["token_type"], "Bearer");

    let init = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}});
    let said: Json = mcp(&h, Some(access), init).await.json().await.unwrap();
    assert_eq!(said["result"]["protocolVersion"], "2025-06-18");
    let res = mcp(
        &h,
        Some(access),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
    assert_eq!(res.status(), 202);
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let said: Json = mcp(&h, Some(access), list).await.json().await.unwrap();
    assert!(said["result"]["tools"].as_array().unwrap().len() > 15);

    // A query, in her group, without saying which.
    let got = tool(&h, access, "group_show", json!({})).await;
    assert_eq!(
        got["structuredContent"]["group"]["did"],
        group.as_str(),
        "{got}"
    );
    // Who is in it, found by part of their DID.
    let got = tool(&h, access, "member_list", json!({"q": &alice.did[8..]})).await;
    let found = &got["structuredContent"]["members"];
    assert_eq!(found[0]["actor"]["did"], alice.did.as_str(), "{got}");
    assert_eq!(found.as_array().unwrap().len(), 1, "{got}");
    let got = tool(
        &h,
        access,
        "invite_list",
        json!({"limit": 5, "also": ["a", "b"]}),
    )
    .await;
    assert!(got.get("isError").is_none(), "{got}");
    // An invite written as the group, as her own session for it would.
    let got = tool(
        &h,
        access,
        "invite_send",
        json!({"person": bob.did, "message": "Ride with us", "expires_at": "2099-01-01T00:00:00Z"}),
    )
    .await;
    assert!(got.get("isError").is_none(), "{got}");
    let uri = got["structuredContent"]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(uri.contains(&format!("/{group}/{NS}.invite/")), "{uri}");
    let got = tool(&h, access, "invite_send", json!({"person": "bob"})).await;
    assert_eq!(got["isError"], true, "{got}");
    let lxm = format!("{NS}.listInvites");
    let (_, invites) = get(
        &h,
        &lxm,
        &format!("group={group}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(
        (
            invites["invites"][0]["invitee"].clone(),
            invites["invites"][0]["uri"].clone()
        ),
        (json!(bob.did), json!(uri)),
        "{invites}"
    );
    // Taken back; then there is none.
    let got = tool(
        &h,
        access,
        "invite_send",
        json!({"person": bob.did, "undo": true}),
    )
    .await;
    assert_eq!(got["structuredContent"]["uri"], uri.as_str(), "{got}");
    let got = tool(
        &h,
        access,
        "invite_send",
        json!({"person": bob.did, "undo": true}),
    )
    .await;
    let said = got["content"][0]["text"].as_str().unwrap();
    assert!(said.contains("InviteNotFound"), "{got}");
    // What the host refuses, it says why.
    let refused = |got: &Json, why: &str| {
        assert_eq!(got["isError"], true, "{got}");
        let said = got["content"][0]["text"].as_str().unwrap();
        assert!(said.starts_with(why), "{got}");
    };
    let got = tool(
        &h,
        access,
        "member_assign",
        json!({"member": carol.did, "roles": ["member"]}),
    )
    .await;
    refused(&got, "JoinRequestNotFound");
    let got = tool(&h, access, "member_eject", json!({"member": alice.did})).await;
    refused(&got, "LastConfigurer");
    let got = tool(&h, access, "role_delete", json!({"role": "member"})).await;
    refused(&got, "RoleInUse");
    // A method of the host's refused says why in its words.
    let got = tool(&h, access, "join_deny", json!({"member": carol.did})).await;
    refused(&got, "JoinRequestNotFound");
    // The group's records, written as the group, as she may: a role, what
    // it may do, and who holds it.
    let got = tool(
        &h,
        access,
        "role_set",
        json!({"role": "leader", "display_name": "Ride leader"}),
    )
    .await;
    let uri = format!(
        "{}/{group}/{NS}.role/leader",
        group_host::group::members_space(&group)
    );
    assert_eq!(got["structuredContent"]["uri"], uri, "{got}");
    let members = group_host::group::members_space(&group);
    let kept = |c: &str, k: &str| {
        h.host
            .group_record(&group, &members, &format!("{NS}.{c}"), k)
            .unwrap()
    };
    let leader = kept("role", "leader").unwrap();
    assert_eq!(leader.get("rkey"), None);
    let grants = |p: Json| p["grants"].as_array().unwrap().clone();
    let before = grants(kept("permissions", "self").unwrap());
    let got = tool(
        &h,
        access,
        "role_grant",
        json!({"role": "leader", "assignable": []}),
    )
    .await;
    assert!(got.get("isError").is_none(), "{got}");
    let mut after = before.clone();
    after.push(json!({"role": "leader", "scopes": [], "assignable": []}));
    assert_eq!(grants(kept("permissions", "self").unwrap()), after);
    // Dropped, it would be a grant of a role she may not assign: refused,
    // as any write of the record is.
    let undo = json!({"role": "leader", "undo": true});
    let got = tool(&h, access, "role_grant", undo).await;
    refused(&got, "NotAssignable");
    assert_eq!(grants(kept("permissions", "self").unwrap()), after);
    // What a tool is not given, it says it wants.
    let got = tool(&h, access, "role_grant", json!({})).await;
    refused(&got, "role is required");
    let lxm = format!("{NS}.requestJoin");
    post(
        &h,
        &lxm,
        &json!({"group": group}),
        &[auth(&h, &carol, &lxm)],
    )
    .await;
    let got = tool(
        &h,
        access,
        "member_assign",
        json!({"member": carol.did, "roles": ["member"]}),
    )
    .await;
    assert!(got.get("isError").is_none(), "{got}");
    assert!(kept("membership", &carol.did).is_some());
    let got = tool(&h, access, "member_eject", json!({"member": carol.did})).await;
    assert!(got.get("isError").is_none(), "{got}");
    assert!(kept("membership", &carol.did).is_none());
    // Another group's, never: the group is the one she chose.
    let other = start_group(
        &h,
        &bob,
        json!({"handle": "hikers.groups.test", "displayName": "Hikers"}),
    )
    .await;
    let got = tool(&h, access, "group_show", json!({"group": other})).await;
    assert_eq!(got["structuredContent"]["group"]["did"], group.as_str());
}

#[tokio::test]
async fn writes_the_groups_profile_and_rules_as_the_group() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let (client, tokens) = connect(&h, &alice, &group).await;
    let access = tokens["access_token"].as_str().unwrap();
    let meta = group_host::group::meta_space(&group);
    let kept = |collection: &str, rkey: &str| {
        let c = format!("{NS}.{collection}");
        h.host.group_record(&group, &meta, &c, rkey).unwrap()
    };

    // The profile, at its one key, replaced whole.
    let profile = json!({"display_name": "Riders United", "join_policy": "open"});
    let got = tool(&h, access, "profile_set", profile).await;
    assert!(got.get("isError").is_none(), "{got}");
    let uri = format!("{meta}/{group}/{NS}.profile/self");
    assert_eq!(got["structuredContent"]["uri"], uri);
    assert!(got["structuredContent"]["cid"].is_string());
    let written = kept("profile", "self").unwrap();
    assert_eq!(
        written,
        json!({"$type": format!("{NS}.profile"), "displayName": "Riders United", "joinPolicy": "open"})
    );

    // A new rule, keyed by the clock; changed at its key; taken away.
    let got = tool(
        &h,
        access,
        "rule_set",
        json!({"title": "Be kind", "text": ""}),
    )
    .await;
    assert!(got.get("isError").is_none(), "{got}");
    let made = got["structuredContent"]["uri"].as_str().unwrap();
    let rkey = made.rsplit('/').next().unwrap().to_string();
    assert_eq!(kept("rule", &rkey).unwrap()["title"], "Be kind");
    let again = json!({"rule": rkey, "title": "Be very kind", "text": "Always."});
    let got = tool(&h, access, "rule_set", again).await;
    assert_eq!(got["structuredContent"]["uri"], made, "{got}");
    assert_eq!(kept("rule", &rkey).unwrap()["title"], "Be very kind");
    let got = tool(&h, access, "rule_delete", json!({"rule": rkey})).await;
    assert_eq!(got["structuredContent"], json!({"uri": made}), "{got}");
    assert_eq!(kept("rule", &rkey), None);

    // Held to what the host keeps, as any write as the group is.
    let got = tool(
        &h,
        access,
        "profile_set",
        json!({"display_name": " ", "join_policy": "open"}),
    )
    .await;
    assert_eq!(got["isError"], true, "{got}");
    let text = got["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("InvalidRequest: "), "{text}");
    let got = tool(
        &h,
        access,
        "rule_set",
        json!({"rule": "chosen", "title": "x", "text": ""}),
    )
    .await;
    assert_eq!(got["isError"], true, "{got}");

    // What it did is in the group's log, as the MCP client's.
    let lxm = format!("{NS}.listGroupTokens");
    let (_, listed) = get(
        &h,
        &lxm,
        &format!("group={group}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    let by = &listed["activity"][0];
    assert_eq!(by["clientId"], client.as_str(), "{listed}");
    assert_eq!(by["member"], alice.did.as_str());
}

#[tokio::test]
async fn does_only_what_the_members_roles_allow() {
    let h = start(&[], false).await;
    let [alice, bob, _] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    // Bob holds no role yet: there is nothing to connect it to.
    let client = register(&h).await;
    let (_, challenge) = pkce();
    let uri = requested(&h, &client, &challenge).await;
    let said = call(
        &h,
        &bob,
        "approveAuthorization",
        json!({"requestUri": uri, "group": group}),
    )
    .await;
    assert_eq!(said.0, 403, "{}", said.1);

    make_member(&h, &group, &bob.did).await;
    let (_, tokens) = connect(&h, &bob, &group).await;
    let access = tokens["access_token"].as_str().unwrap();
    let got = tool(&h, access, "rule_set", json!({"title": "No spam"})).await;
    assert_eq!(got["isError"], true, "{got}");
    // Nor may it write what his roles do not: he may give nobody roles.
    let got = tool(
        &h,
        access,
        "member_assign",
        json!({"member": bob.did, "roles": ["member", "admin"]}),
    )
    .await;
    assert_eq!(got["isError"], true, "{got}");
    // Put out, his MCP client acts as him there no more.
    let runs = common::members::Session::runs(&h, &alice, &group).await;
    let said = runs.delete(&h, "membership", &bob.did).await;
    assert_eq!(said.status, 200, "{}", said.body);
    let res = mcp(
        &h,
        Some(access),
        json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}),
    )
    .await;
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn refreshes_lists_and_disconnects() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let (client, tokens) = connect(&h, &alice, &group).await;
    let refresh = tokens["refresh_token"].as_str().unwrap();
    let (status, fresh) = token(
        &h,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", &client),
        ],
    )
    .await;
    assert_eq!(status, 200, "{fresh}");
    // A refresh token is used once, and only by its client.
    let (status, said) = token(
        &h,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", &client),
        ],
    )
    .await;
    assert_eq!(
        (status, said["error"].as_str().unwrap()),
        (400, "invalid_grant")
    );
    let next = fresh["refresh_token"].as_str().unwrap();
    let (status, _) = token(
        &h,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", next),
            ("client_id", "another"),
        ],
    )
    .await;
    assert_eq!(status, 400);
    let access = fresh["access_token"].as_str().unwrap();
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
    assert_eq!(mcp(&h, Some(access), ping.clone()).await.status(), 200);

    let lxm = format!("{NS}.listConnectedApps");
    let (status, listed) = get(
        &h,
        &lxm,
        &format!("group={group}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["apps"][0]["name"], "Claude");
    assert_eq!(listed["apps"][0]["clientId"], client.as_str());
    assert!(listed["apps"][0]["usedAt"].is_string());

    let said = call(
        &h,
        &alice,
        "revokeConnectedApp",
        json!({"group": group, "clientId": client}),
    )
    .await;
    assert_eq!(said.0, 200, "{}", said.1);
    assert_eq!(mcp(&h, Some(access), ping).await.status(), 401);
    let (_, listed) = get(
        &h,
        &lxm,
        &format!("group={group}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(listed["apps"], json!([]));
    for body in [json!({"clientId": client}), json!({"group": group})] {
        assert_eq!(call(&h, &alice, "revokeConnectedApp", body).await.0, 400);
    }
}

#[tokio::test]
async fn sends_a_declined_mcp_client_back_saying_so() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let client = register(&h).await;
    let (_, challenge) = pkce();
    let uri = requested(&h, &client, &challenge).await;
    let (status, declined) = call(
        &h,
        &alice,
        "declineAuthorization",
        json!({"requestUri": uri}),
    )
    .await;
    assert_eq!(status, 200, "{declined}");
    let back = query(declined["redirectUrl"].as_str().unwrap());
    assert_eq!(back["error"], "access_denied");
    assert_eq!(back["state"], "s1");
    // Answered, it is gone.
    for (method, body) in [
        (
            "approveAuthorization",
            json!({"requestUri": uri, "group": group}),
        ),
        ("declineAuthorization", json!({"requestUri": uri})),
    ] {
        let said = call(&h, &alice, method, body).await;
        assert_eq!(said.1["error"], "ExpiredRequest", "{method}");
    }
    let lxm = format!("{NS}.getAuthorization");
    let (_, said) = get(
        &h,
        &lxm,
        &format!("requestUri={uri}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(said["error"], "ExpiredRequest");
}

#[tokio::test]
async fn refuses_a_request_it_cannot_trust() {
    let h = start(&[], false).await;
    let client = register(&h).await;
    let (verifier, challenge) = pkce();
    // Not knowing whom to send back, it says so to the browser.
    for extra in [
        &[("client_id", "")][..],
        &[("client_id", "nobody")],
        &[("redirect_uri", "https://evil.test/cb")],
    ] {
        let res = authorize(&h, &client, &challenge, extra).await;
        assert_eq!(res.status(), 400, "{extra:?}");
    }
    // Knowing, it sends the client back with why.
    for (extra, error) in [
        (
            &[("response_type", "token")][..],
            "unsupported_response_type",
        ),
        (&[("code_challenge_method", "plain")], "invalid_request"),
        (&[("code_challenge", "short")], "invalid_request"),
        (&[("resource", "https://other.test/mcp")], "invalid_request"),
        (&[("state", "")], "invalid_request"),
    ] {
        let mut extra = extra.to_vec();
        if error == "invalid_request" && extra[0].0 == "state" {
            extra.push(("code_challenge", "short"));
        }
        let res = authorize(&h, &client, &challenge, &extra).await;
        assert_eq!(res.status(), 303);
        let back = query(&location(&res));
        assert_eq!(back["error"], error, "{extra:?}");
    }

    // A code once, to its client, its redirect URI and its verifier.
    let [alice, ..] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let uri = requested(&h, &client, &challenge).await;
    let (_, approved) = call(
        &h,
        &alice,
        "approveAuthorization",
        json!({"requestUri": uri, "group": group}),
    )
    .await;
    let code = query(approved["redirectUrl"].as_str().unwrap())["code"].clone();
    let exchange = |client: &str, verifier: &str| {
        vec![
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".into(), code.clone()),
            ("redirect_uri".into(), BACK.into()),
            ("code_verifier".into(), verifier.into()),
            ("client_id".into(), client.into()),
        ]
    };
    let form = exchange(
        &client,
        "a-verifier-that-is-long-enough-but-not-the-one-it-was-made-for",
    );
    let form: Vec<(&str, &str)> = form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (status, said) = token(&h, &form).await;
    assert_eq!(
        (status, said["error"].as_str().unwrap()),
        (400, "invalid_grant")
    );
    // Tried once, it is spent.
    let form = exchange(&client, &verifier);
    let form: Vec<(&str, &str)> = form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (status, _) = token(&h, &form).await;
    assert_eq!(status, 400);
    let (status, said) = token(&h, &[("grant_type", "password")]).await;
    assert_eq!(
        (status, said["error"].as_str().unwrap()),
        (400, "unsupported_grant_type")
    );

    // Registered only to be sent back where it may be.
    let many: Vec<String> = (0..11).map(|i| format!("https://app.test/{i}")).collect();
    for uris in [
        json!(many),
        json!([format!("https://app.test/{}", "a".repeat(2048))]),
        json!(["javascript:alert(1)"]),
        json!(["http://evil.test/cb"]),
        json!(["https://app.test/cb#x"]),
        json!(["http://127.0.0.1/cb#x"]),
        json!(["cursor://anysphere.cursor-retrieval/oauth/callback#x"]),
        json!(["not a url"]),
        json!([]),
    ] {
        let res = h
            .http
            .post(format!("{}/mcp/register", h.url))
            .json(&json!({"redirect_uris": uris}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "{uris}");
    }
    let res = h
        .http
        .post(format!("{}/mcp/register", h.url))
        .json(&json!({"redirect_uris": ["cursor://anysphere.cursor-retrieval/oauth/callback"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let said: Json = res.json().await.unwrap();
    assert_eq!(said["client_name"], "An AI app");
}

/// A client metadata document, served at a URL `make` is given: the URL.
async fn served(make: impl Fn(&str) -> Json) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/client.json", listener.local_addr().unwrap());
    let doc = make(&url);
    let router: Router = Router::new().route(
        "/client.json",
        route(move || {
            let doc = doc.clone();
            async move { axum::Json(doc) }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    url
}

#[tokio::test]
async fn takes_a_client_by_its_metadata_document() {
    // A host on http, as on a dev network, takes a document over http.
    let h = start_reachable(&[]).await;
    let (_, challenge) = pkce();
    let id =
        served(|url| json!({"client_id": url, "client_name": "Desk", "redirect_uris": [BACK]}))
            .await;
    let res = authorize(&h, &id, &challenge, &[("resource", "")]).await;
    assert_eq!(res.status(), 303);
    let uri = query(&location(&res))["request_uri"].clone();
    let [alice, ..] = people(&h).await;
    let lxm = format!("{NS}.getAuthorization");
    let (_, view) = get(
        &h,
        &lxm,
        &format!("requestUri={uri}"),
        &[auth(&h, &alice, &lxm)],
    )
    .await;
    assert_eq!(view["client"]["name"], "Desk");
    // One that names another client, or no way back, is no client.
    for doc in [
        served(
            |_| json!({"client_id": "http://elsewhere.test/client.json", "redirect_uris": [BACK]}),
        )
        .await,
        served(|url| json!({"client_id": url})).await,
        // One it may not be sent back to, among them.
        served(|url| json!({"client_id": url, "redirect_uris": [BACK, "javascript:alert(1)"]}))
            .await,
    ] {
        let res = authorize(&h, &doc, &challenge, &[("resource", "")]).await;
        assert_eq!(res.status(), 400, "{doc}");
    }
}

#[tokio::test]
async fn says_what_it_cannot_do_as_json_rpc_does() {
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let (_, tokens) = connect(&h, &alice, &group).await;
    let access = tokens["access_token"].as_str().unwrap();
    let (h, access) = (&h, access);
    let ask = async |message: Json| {
        let said: Json = mcp(h, Some(access), message).await.json().await.unwrap();
        said["error"]["code"].as_i64()
    };
    let call =
        |params: Json| json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": params});
    assert_eq!(ask(json!("not a message")).await, Some(-32600));
    // Not JSON at all: a parse error, of no message's id.
    let res = h
        .http
        .post(format!("{}/mcp", h.url))
        .bearer_auth(access)
        .header("content-type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let said: Json = res.json().await.unwrap();
    assert_eq!(
        (said["error"]["code"].as_i64(), &said["id"]),
        (Some(-32700), &Json::Null)
    );
    assert_eq!(
        ask(json!({"jsonrpc": "2.0", "id": 1, "method": "resources/list"})).await,
        Some(-32601)
    );
    assert_eq!(ask(call(json!({"name": "nothing"}))).await, Some(-32602));
    assert_eq!(
        ask(call(json!({"name": "group_show", "arguments": 1}))).await,
        Some(-32602)
    );
    // No arguments at all is none to give.
    let said: Json = mcp(h, Some(access), call(json!({"name": "group_show"})))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        said["result"]["structuredContent"]["group"]["did"],
        group.as_str()
    );
}

#[tokio::test]
async fn forgets_a_client_that_never_connected() {
    let h = start(&[], false).await;
    let store = &h.host.store;
    let day = 24 * 60 * 60_000;
    let now = 10 * day;
    let back = vec![BACK.to_string()];
    store
        .register_mcp_client("old", "Old", &back, now - 2 * day)
        .unwrap();
    store.register_mcp_client("new", "New", &back, now).unwrap();
    assert!(store.registered_mcp_client("old").unwrap().is_none());
    assert!(store.registered_mcp_client("new").unwrap().is_some());
}

#[tokio::test]
async fn keeps_a_client_once_connected_and_forgets_a_lapsed_connection() {
    use group_host::mcp::McpGrant;
    let h = start(&[], false).await;
    let [alice, ..] = people(&h).await;
    let group = start_group(
        &h,
        &alice,
        json!({"handle": RIDERS, "displayName": "Riders"}),
    )
    .await;
    let store = &h.host.store;
    let day = 24 * 60 * 60_000;
    let now = 100 * day;
    let back = vec![BACK.to_string()];
    let connect = |id: &str, client: &str, at: i64| {
        let c = McpGrant {
            id: id.into(),
            group: group.clone(),
            member: alice.did.clone(),
            client_id: client.into(),
            client_name: "Kept".into(),
            created_at: at,
            used_at: at,
        };
        store
            .connect_mcp_client(&c, &format!("a-{id}"), &format!("r-{id}"), at)
            .unwrap();
    };
    // Connected, then disconnected: kept, to connect again.
    store
        .register_mcp_client("kept", "Kept", &back, now - 2 * day)
        .unwrap();
    connect("one", "kept", now - 2 * day);
    store
        .disconnect_mcp_client(&group, &alice.did, "kept")
        .unwrap();
    // A connection whose refresh token lapsed long ago.
    connect("two", "lapsed", now - 40 * day);
    store.register_mcp_client("new", "New", &back, now).unwrap();
    assert!(store.registered_mcp_client("kept").unwrap().is_some());
    let lapsed = store.disconnect_mcp_client(&group, &alice.did, "lapsed");
    assert_eq!(lapsed.unwrap(), 0);
}
