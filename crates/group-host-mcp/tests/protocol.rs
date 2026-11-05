//! The tools, as an MCP client lists them, what each call comes to, and what
//! the server answers each message an MCP client sends.

use group_host_mcp::{
    Action, FAVICONS, Key, PROTOCOL_VERSIONS, Plan, Tool, answer, camel, edited, iconed, iconless,
    plan, regrant, snake, tools,
};
use serde_json::{Value, json};

/// What the server answers `message`, each tool answering as `said` says.
async fn ask(message: Value, said: Result<Value, String>) -> Option<Value> {
    let tools = tools();
    answer(&message, &tools, |_: Tool, _: Value| async move { said }).await
}

fn tool(name: &str) -> Tool {
    tools().into_iter().find(|t| t.name == name).unwrap()
}

const NOW: &str = "2026-11-04T00:00:00.000Z";

/// What calling `name` with `args` comes to.
fn planned(name: &str, args: Value) -> Result<Plan, String> {
    plan(&tool(name), &args, NOW)
}

#[test]
fn names_each_tool_for_its_command() {
    let names: Vec<String> = tools().into_iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "group_show",
            "group_deactivate",
            "profile_set",
            "rule_set",
            "rule_delete",
            "role_set",
            "role_delete",
            "role_grant",
            "site_publish",
            "site_delete",
            "member_list",
            "member_assign",
            "member_eject",
            "join_list",
            "join_deny",
            "invite_send",
            "invite_list",
            "space_create",
            "space_records",
            "mod_list",
            "mod_history",
            "mod_resolve",
            "mod_label",
        ]
    );
    assert_eq!(tool("member_assign").command(), "member assign");
    for t in tools() {
        let properties = t.input_schema["properties"].as_object().unwrap();
        // Never the group, which is the one the MCP client was connected to.
        assert!(properties.get("group").is_none(), "{}", t.name);
        // Named as its command's flags are, in snake case.
        for name in properties.keys() {
            assert_eq!(name, &snake(name), "{}", t.name);
        }
        // What its command takes in order, it requires.
        let required = t.input_schema["required"].as_array().unwrap();
        for arg in &t.args {
            assert!(required.iter().any(|r| r == arg), "{}: {arg}", t.name);
        }
        assert!(t.description.len() > 20, "{}", t.name);
    }
    assert_eq!(snake("expiresAt"), "expires_at");
    assert_eq!(camel("expires_at"), "expiresAt");
}

#[test]
fn makes_each_schema_from_the_lexicons() {
    let show = tool("group_show");
    assert_eq!(
        show.input_schema,
        json!({"type": "object", "properties": {}, "required": []})
    );
    assert!(
        show.description
            .starts_with("The group as the member may see it. Get a detailed view")
    );

    // A membership, at its member's DID; each role its id.
    let assign = tool("member_assign");
    assert_eq!(assign.args, ["member", "roles"]);
    let fields = &assign.input_schema["properties"];
    assert_eq!(fields["member"]["format"], "did");
    assert_eq!(fields["roles"]["items"]["type"], "string");
    assert!(fields.get("created_at").is_none());
    assert_eq!(
        assign.action,
        Action::Put {
            collection: "fyi.opensocial.membership".into(),
            space: "fyi.opensocial.members".into(),
            key: Key::Field("member".into()),
            stamp: true,
        }
    );
    let eject = tool("member_eject");
    assert_eq!(eject.input_schema["required"], json!(["member"]));
    assert_eq!(eject.input_schema["properties"]["member"]["format"], "did");

    // The profile, at its one key: none to say, nor when it was made;
    // each field changed only if given.
    let profile = tool("profile_set");
    let fields = &profile.input_schema["properties"];
    assert_eq!(fields["display_name"]["maxLength"], 640);
    assert!(fields.get("rkey").is_none() && fields.get("created_at").is_none());
    assert_eq!(profile.input_schema["required"], json!([]));
    assert!(
        profile
            .description
            .contains("Changes only the fields given")
    );
    let said = fields["join_policy"]["description"].as_str().unwrap();
    assert!(said.contains("One of: "), "{said}");
    assert_eq!(fields["online"]["type"], "boolean");
    assert!(profile.description.contains("The group's profile"));

    // A rule to change, or none for a new one; one to delete.
    let rule = tool("rule_set");
    assert_eq!(
        rule.input_schema["properties"]["rule"]["format"],
        "record-key"
    );
    assert_eq!(rule.input_schema["required"], json!(["title", "text"]));
    assert_eq!(
        tool("rule_delete").input_schema["required"],
        json!(["rule"])
    );
    assert_eq!(tool("role_delete").args, ["role"]);

    // A role's grant: its scopes plain strings; with undo, dropped.
    let grant = tool("role_grant");
    let fields = &grant.input_schema["properties"];
    assert_eq!(fields["scopes"]["items"]["type"], "string");
    assert_eq!(fields["assignable"]["items"]["type"], "string");
    assert_eq!(fields["undo"]["type"], "boolean");
    assert_eq!(grant.input_schema["required"], json!(["role"]));

    // An invite: the record's fields but when it was made, and whom for.
    let send = tool("invite_send");
    let fields = send.input_schema["properties"].as_object().unwrap();
    let mut names: Vec<&str> = fields.keys().map(String::as_str).collect();
    names.sort();
    assert_eq!(names, ["expires_at", "message", "person", "undo"]);
    assert_eq!(send.input_schema["required"], json!(["person"]));
    assert_eq!(fields["message"]["maxLength"], 20000);

    // A closed set, as JSON Schema's own; limits as the lexicon's.
    let fields = &tool("mod_list").input_schema["properties"];
    assert_eq!(
        fields["status"]["enum"],
        json!(["open", "escalated", "resolved"])
    );
    assert_eq!(fields["limit"]["type"], "integer");
    assert!(fields["limit"].get("maximum").is_some());

    // A subject, an account or a record, as text; and its opposite.
    let label = tool("mod_label");
    let fields = &label.input_schema["properties"];
    assert_eq!(fields["subject"]["type"], "string");
    assert_eq!(fields["cid"]["format"], "cid");
    assert_eq!(fields["undo"]["type"], "boolean");
    assert_eq!(label.args, ["subject", "val"]);
    let history = &tool("mod_history").input_schema["properties"]["subject"];
    assert_eq!(history["format"], "uri");
    assert!(
        tool("mod_resolve").input_schema["properties"]
            .get("undo")
            .is_none()
    );
    assert_eq!(
        tool("space_records").input_schema["required"],
        json!(["space"])
    );
}

#[test]
fn plans_a_method_asked_with_its_arguments() {
    assert_eq!(
        planned("group_show", json!({})),
        Ok(Plan::Call {
            nsid: "fyi.opensocial.getGroup".into(),
            procedure: false,
            input: json!({}),
        })
    );
    // Its opposite, with undo, which takes nothing but the group.
    let off = |undo: bool| planned("group_deactivate", json!({"undo": undo}));
    let call = |nsid: &str| Plan::Call {
        nsid: format!("fyi.opensocial.{nsid}"),
        procedure: true,
        input: json!({}),
    };
    assert_eq!(off(false), Ok(call("deactivateGroup")));
    assert_eq!(off(true), Ok(call("activateGroup")));
    let Ok(Plan::Call { input, .. }) = planned("join_list", json!({"limit": 5})) else {
        panic!()
    };
    assert_eq!(input, json!({"limit": 5}));
    // An account, or a record at a version; the opposite asked instead.
    let call = |args: Value| match planned("mod_label", args).unwrap() {
        Plan::Call { nsid, input, .. } => (nsid, input),
        other => panic!("{other:?}"),
    };
    let (nsid, input) = call(json!({"subject": "did:plc:a", "val": "spam", "undo": false}));
    assert_eq!(nsid, "fyi.opensocial.applyLabel");
    assert_eq!(
        input,
        json!({"subject": {"$type": "com.atproto.admin.defs#repoRef", "did": "did:plc:a"}, "val": "spam"})
    );
    let post = "at://did:plc:g/space/x/self/did:plc:a/com.example.post/1";
    let (nsid, input) = call(json!({"subject": post, "cid": "bafy", "val": "spam", "undo": true}));
    assert_eq!(nsid, "fyi.opensocial.negateLabel");
    assert_eq!(
        input["subject"],
        json!({"$type": "com.atproto.repo.strongRef", "uri": post, "cid": "bafy"})
    );
    let Ok(Plan::Call { input, .. }) = planned(
        "mod_resolve",
        json!({"subject": post, "status": "resolved"}),
    ) else {
        panic!()
    };
    assert_eq!(
        input["subject"],
        json!({"$type": "fyi.opensocial.defs#recordSubject", "uri": post})
    );
    // Nothing to make a subject of, nothing made.
    let (_, input) = call(json!({"val": "spam"}));
    assert_eq!(input, json!({"val": "spam"}));
}

#[test]
fn plans_a_record_written_as_the_group() {
    let write = |space: &str, kind: &str, rkey: Option<&str>, record: Option<Value>| Plan::Write {
        space: format!("fyi.opensocial.{space}"),
        collection: format!("fyi.opensocial.{kind}"),
        rkey: rkey.map(String::from),
        record,
    };
    // In snake case, in the lexicon's own case: the fields to change.
    assert_eq!(
        planned(
            "profile_set",
            json!({"display_name": "Riders", "description": ""})
        ),
        Ok(Plan::Edit {
            space: "fyi.opensocial.meta".into(),
            collection: "fyi.opensocial.profile".into(),
            rkey: "self".into(),
            fields: json!({"displayName": "Riders", "description": ""})
                .as_object()
                .unwrap()
                .clone(),
        })
    );
    let rule = json!({"$type": "fyi.opensocial.rule", "title": "Be kind", "text": ""});
    assert_eq!(
        planned("rule_set", json!({"title": "Be kind", "text": ""})),
        Ok(write("meta", "rule", None, Some(rule.clone())))
    );
    assert_eq!(
        planned(
            "rule_set",
            json!({"rule": "3k", "title": "Be kind", "text": ""})
        ),
        Ok(write("meta", "rule", Some("3k"), Some(rule)))
    );
    assert_eq!(
        planned("rule_delete", json!({"rule": "3k"})),
        Ok(write("meta", "rule", Some("3k"), None))
    );
    assert_eq!(
        planned("rule_delete", json!({"rule": 3})),
        Err("rule is required, as text".into())
    );
    assert_eq!(
        planned(
            "role_set",
            json!({"role": "leader", "display_name": "Ride leader"})
        ),
        Ok(write(
            "members",
            "role",
            Some("leader"),
            Some(json!({"$type": "fyi.opensocial.role", "displayName": "Ride leader"}))
        ))
    );
    // A membership at its member's DID, saying when it was made.
    assert_eq!(
        planned(
            "member_assign",
            json!({"member": "did:plc:b", "roles": ["member"]})
        ),
        Ok(write(
            "members",
            "membership",
            Some("did:plc:b"),
            Some(
                json!({"$type": "fyi.opensocial.membership", "member": "did:plc:b", "roles": ["member"], "createdAt": NOW})
            )
        ))
    );
    assert_eq!(
        planned("member_eject", json!({"member": "did:plc:b"})),
        Ok(write("members", "membership", Some("did:plc:b"), None))
    );
    assert_eq!(
        planned("member_eject", json!({})),
        Err("member is required, as text".into())
    );
}

#[test]
fn plans_an_invite_and_a_grant() {
    assert_eq!(
        planned(
            "invite_send",
            json!({"person": "did:plc:b", "message": "Ride with us", "expires_at": "2099-01-01T00:00:00Z"})
        ),
        Ok(Plan::Invite {
            person: "did:plc:b".into(),
            record: Some(
                json!({"$type": "fyi.opensocial.invite", "message": "Ride with us", "expiresAt": "2099-01-01T00:00:00Z", "createdAt": NOW})
            ),
        })
    );
    assert_eq!(
        planned("invite_send", json!({"person": "did:plc:b", "undo": true})),
        Ok(Plan::Invite {
            person: "did:plc:b".into(),
            record: None
        })
    );
    assert_eq!(
        planned("invite_send", json!({})),
        Err("person is required, as text".into())
    );
    assert_eq!(
        planned("role_grant", json!({"role": "leader", "scopes": ["rpc:x"]})),
        Ok(Plan::Grant {
            role: "leader".into(),
            scopes: vec!["rpc:x".into()],
            assignable: vec![],
            undo: false,
        })
    );
    assert_eq!(
        planned(
            "role_grant",
            json!({"role": "leader", "assignable": ["member"], "undo": true})
        ),
        Ok(Plan::Grant {
            role: "leader".into(),
            scopes: vec![],
            assignable: vec!["member".into()],
            undo: true,
        })
    );
    assert!(planned("role_grant", json!({})).is_err());
}

#[test]
fn regrants_one_role_keeping_the_rest() {
    let grant = |role: &str, scopes: &[&str], assignable: &[&str]| json!({"role": role, "scopes": scopes, "assignable": assignable});
    let written = |plan: Plan| match plan {
        Plan::Write {
            space,
            collection,
            rkey,
            record,
        } => {
            assert_eq!(
                (space.as_str(), collection.as_str(), rkey.as_deref()),
                (
                    "fyi.opensocial.members",
                    "fyi.opensocial.permissions",
                    Some("self")
                )
            );
            record.unwrap()
        }
        other => panic!("{other:?}"),
    };
    let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let kept = json!({"$type": "fyi.opensocial.permissions", "grants": [grant("admin", &["a"], &[]), grant("leader", &["b"], &["member"]), grant("leader", &["c"], &[])], "createdAt": NOW});
    // What is named is added to what the role had, once, in its place;
    // the rest as they were.
    let record = written(regrant(&kept, "leader", &names(&["d", "b"]), &[], false));
    assert_eq!(
        record["grants"],
        json!([
            grant("admin", &["a"], &[]),
            grant("leader", &["b", "c", "d"], &["member"])
        ])
    );
    assert_eq!(record["createdAt"], NOW);
    // With undo, what is named is taken away; with nothing named, or
    // nothing left, the grant goes.
    let record = written(regrant(&kept, "leader", &names(&["b"]), &[], true));
    assert_eq!(
        record["grants"],
        json!([
            grant("admin", &["a"], &[]),
            grant("leader", &["c"], &["member"])
        ])
    );
    let record = written(regrant(&kept, "leader", &[], &[], true));
    assert_eq!(record["grants"], json!([grant("admin", &["a"], &[])]));
    let record = written(regrant(&kept, "admin", &names(&["a"]), &[], true));
    let roles: Vec<&Value> = record["grants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| &g["role"])
        .collect();
    assert_eq!(roles, ["leader", "leader"]);
    let record = written(regrant(&kept, "mod", &names(&["e"]), &[], false));
    assert_eq!(record["grants"][3], grant("mod", &["e"], &[]));
    // A group with none yet has one.
    let record = written(regrant(&Value::Null, "mod", &names(&["e"]), &[], false));
    assert_eq!(
        record,
        json!({"$type": "fyi.opensocial.permissions", "grants": [grant("mod", &["e"], &[])]})
    );
}

#[test]
fn edits_a_record_keeping_what_is_not_given() {
    let avatar =
        json!({"$type": "blob", "ref": {"$link": "bafy"}, "mimeType": "image/png", "size": 3});
    let kept = json!({"$type": "fyi.opensocial.profile", "displayName": "Riders", "description": "Bikes", "avatar": avatar, "links": [{"url": "https://a.test"}], "joinPolicy": "open"});
    let fields = json!({"displayName": "Riders United", "description": "", "links": [], "url": null, "online": false});
    let plan = edited(
        &kept,
        "fyi.opensocial.meta".into(),
        "fyi.opensocial.profile".into(),
        "self".into(),
        fields.as_object().unwrap().clone(),
    );
    assert_eq!(
        plan,
        Plan::Write {
            space: "fyi.opensocial.meta".into(),
            collection: "fyi.opensocial.profile".into(),
            rkey: Some("self".into()),
            record: Some(
                json!({"$type": "fyi.opensocial.profile", "displayName": "Riders United", "avatar": avatar, "joinPolicy": "open", "online": false})
            ),
        }
    );
    // None yet: only what is given.
    let fields = json!({"displayName": "Riders"});
    let plan = edited(
        &Value::Null,
        "fyi.opensocial.meta".into(),
        "fyi.opensocial.profile".into(),
        "self".into(),
        fields.as_object().unwrap().clone(),
    );
    let Plan::Write { record, .. } = plan else {
        panic!()
    };
    assert_eq!(
        record,
        Some(json!({"$type": "fyi.opensocial.profile", "displayName": "Riders"}))
    );
}
#[tokio::test]
async fn answers_initialize_ping_and_the_list() {
    let said = ask(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}}),
        Ok(json!({})),
    )
    .await
    .unwrap();
    assert_eq!(said["id"], 1);
    assert_eq!(said["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(
        said["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    assert!(said["result"]["instructions"].is_string());
    // One it does not speak: the latest it does.
    let said = ask(
        json!({"jsonrpc": "2.0", "id": "a", "method": "initialize", "params": {"protocolVersion": "1999-01-01"}}),
        Ok(json!({})),
    )
    .await
    .unwrap();
    assert_eq!(said["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);

    let said = ask(
        json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}),
        Ok(json!({})),
    )
    .await;
    assert_eq!(said.unwrap()["result"], json!({}));
    let said = ask(
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
        Ok(json!({})),
    )
    .await;
    let listed = said.unwrap()["result"]["tools"].clone();
    assert_eq!(listed.as_array().unwrap().len(), tools().len());
    assert!(listed[0]["inputSchema"]["type"] == "object");

    // A notification is answered by nothing.
    let said = ask(
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        Ok(json!({})),
    )
    .await;
    assert!(said.is_none());
}

#[tokio::test]
async fn calls_a_tool_and_says_what_it_did_or_why_not() {
    let call = |args: Value| json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "group_show", "arguments": args}});
    let said = ask(
        call(Value::Null),
        Ok(json!({"group": {"did": "did:plc:x"}})),
    )
    .await
    .unwrap();
    assert_eq!(
        said["result"]["structuredContent"]["group"]["did"],
        "did:plc:x"
    );
    let text = said["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap()["group"]["did"],
        "did:plc:x"
    );
    assert!(said["result"].get("isError").is_none());

    let said = ask(call(json!({})), Ok(json!([1, 2]))).await.unwrap();
    assert!(said["result"].get("structuredContent").is_none());
    assert_eq!(said["result"]["content"][0]["text"], "[1,2]");

    let said = ask(call(json!({})), Err("Forbidden: missing invite".into()))
        .await
        .unwrap();
    assert_eq!(said["result"]["isError"], true);
    assert_eq!(
        said["result"]["content"][0]["text"],
        "Forbidden: missing invite"
    );

    // The arguments it was given, to the call.
    let tools = tools();
    let message = call(json!({"x": 1}));
    let said = answer(&message, &tools, |t: Tool, args: Value| async move {
        Ok(json!({"tool": t.name, "args": args}))
    })
    .await
    .unwrap();
    assert_eq!(
        said["result"]["structuredContent"],
        json!({"tool": "group_show", "args": {"x": 1}})
    );
}

#[tokio::test]
async fn refuses_what_is_not_a_request_it_knows() {
    let error = |said: Option<Value>| said.unwrap()["error"]["code"].as_i64().unwrap();
    assert_eq!(error(ask(json!([1]), Ok(json!({}))).await), -32600);
    assert_eq!(
        error(
            ask(
                json!({"jsonrpc": "1.0", "id": 1, "method": "ping"}),
                Ok(json!({}))
            )
            .await
        ),
        -32600
    );
    assert_eq!(
        error(
            ask(
                json!({"jsonrpc": "2.0", "id": 1, "method": "resources/list"}),
                Ok(json!({}))
            )
            .await
        ),
        -32601
    );
    assert_eq!(
        error(
            ask(
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "nothing"}}),
                Ok(json!({}))
            )
            .await
        ),
        -32602
    );
    assert_eq!(
        error(
            ask(
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "group_show", "arguments": [1]}}),
                Ok(json!({}))
            )
            .await
        ),
        -32602
    );
}

#[test]
fn asks_the_icon_of_each_new_link_once() {
    let profile = "fyi.opensocial.profile";
    let icon =
        json!({"$type": "blob", "ref": {"$link": "bafkreia"}, "mimeType": "image/png", "size": 9});
    let kept = json!({"links": [
        {"url": "https://chat.example", "icon": icon},
        {"url": "https://old.example"},
    ]});
    let given = || {
        json!({"links": [
            {"url": "https://chat.example", "title": "Our chat"},
            {"url": "https://new.example/a b?c=d"},
            {"url": "https://own.example", "icon": "its own"},
            {"url": "ftp://files.example"},
            "not a link",
            {"url": "https://old.example"},
        ]})
        .as_object()
        .unwrap()
        .clone()
    };
    // A link it had keeps its icon; one new to it, on the web, is asked
    // for, its address encoded; any other is left as given.
    let mut fields = given();
    let wanted = iconless(profile, &kept, &mut fields, FAVICONS);
    let asked = format!("{FAVICONS}https%3A%2F%2Fnew.example%2Fa%20b%3Fc%3Dd");
    let old = format!("{FAVICONS}https%3A%2F%2Fold.example");
    assert_eq!(wanted, vec![(1, asked), (5, old)]);
    assert_eq!(fields["links"][0]["icon"], icon);
    assert_eq!(fields["links"][2]["icon"], "its own");
    assert_eq!(fields["links"][3].get("icon"), None);
    // Given one, it holds it; at no link, nothing is.
    iconed(&mut fields, 1, json!("the icon"));
    iconed(&mut fields, 9, json!("nowhere"));
    assert_eq!(fields["links"][1]["icon"], "the icon");
    assert_eq!(fields["links"].as_array().unwrap().len(), 6);
    // With no service, none is asked; nor of a record but a profile.
    assert_eq!(iconless(profile, &kept, &mut given(), ""), vec![]);
    let rule = "fyi.opensocial.rule";
    assert_eq!(iconless(rule, &kept, &mut given(), FAVICONS), vec![]);
    // Nor of a profile with no links given.
    let mut none = json!({"description": "x"}).as_object().unwrap().clone();
    assert_eq!(iconless(profile, &kept, &mut none, FAVICONS), vec![]);
    iconed(&mut none, 0, json!("nowhere"));
    assert_eq!(none.get("links"), None);
}
