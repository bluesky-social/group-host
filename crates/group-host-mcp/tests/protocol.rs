//! The tools, as an MCP client lists them, what each call comes to, and what
//! the server answers each message an MCP client sends.

use group_host_mcp::{
    Action, Key, LABEL, PROTOCOL_VERSIONS, Plan, Tool, answer, camel, edited, label_record,
    label_space, labels, plan, regrant, snake, tools,
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
            "group_handle",
            "group_domain",
            "profile_set",
            "rule_set",
            "rule_delete",
            "label_define",
            "label_delete",
            "role_set",
            "role_delete",
            "role_grant",
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
            collection: "org.atgrp.membership".into(),
            space: "org.atgrp.members".into(),
            key: Key::Named("member".into()),
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
    assert_eq!(
        tool("role_set").input_schema["properties"]["bot"]["type"],
        "boolean"
    );
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
    // A role's scopes and the roles it may give, as its record has them.
    let set = tool("role_set");
    let fields = &set.input_schema["properties"];
    assert_eq!(fields["scopes"]["maxItems"], 100);
    assert_eq!(fields["assignable"]["items"]["type"], "string");

    // A role's grant: its scopes plain strings; with undo, dropped.
    let grant = tool("role_grant");
    let fields = &grant.input_schema["properties"];
    assert_eq!(fields["role"]["format"], "record-key");
    assert_eq!(fields["scopes"]["items"]["type"], "string");
    assert_eq!(fields["assignable"]["items"]["type"], "string");
    assert_eq!(fields["undo"]["type"], "boolean");
    assert_eq!(grant.input_schema["required"], json!(["role"]));
    assert!(
        grant
            .description
            .contains("A group role and its group-wide grants")
    );

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
    assert_eq!(label.action, Action::Label);
    let fields = &label.input_schema["properties"];
    assert_eq!(fields["subject"]["type"], "string");
    assert_eq!(fields["cid"]["format"], "cid");
    assert_eq!(fields["val"]["maxLength"], 128);
    assert_eq!(fields["undo"]["type"], "boolean");
    assert_eq!(label.input_schema["required"], json!(["subject", "val"]));
    assert_eq!(label.args, ["subject", "val"]);
    assert!(label.description.contains("com.atproto.label.record"));
    // The methods' own subject, a DID or a URI, with its version apart.
    let resolve = &tool("mod_resolve").input_schema["properties"];
    assert_eq!(resolve["subject"]["format"], "uri");
    assert_eq!(resolve["cid"]["format"], "cid");
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
fn plans_a_label_beside_its_subject() {
    let label = |args: Value| planned("mod_label", args);
    assert_eq!(
        label(json!({"subject": "did:plc:a", "val": "spam"})),
        Ok(Plan::Label {
            subject: "did:plc:a".into(),
            cid: None,
            val: "spam".into(),
            undo: false,
        })
    );
    let post = "at://did:plc:g/space/x.y/rides/did:plc:a/com.example.post/1";
    assert_eq!(
        label(json!({"subject": post, "cid": "bafy", "val": "spam", "undo": true})),
        Ok(Plan::Label {
            subject: post.into(),
            cid: Some("bafy".into()),
            val: "spam".into(),
            undo: true,
        })
    );
    assert!(label(json!({"val": "spam"})).is_err());
    assert!(label(json!({"subject": post})).is_err());

    // Where it goes: an account's in the members space, a record's in its
    // own, the group's.
    let members = "at://did:plc:g/space/org.atgrp.members/self";
    assert_eq!(
        label_space("did:plc:g", "did:plc:a").as_deref(),
        Some(members)
    );
    assert_eq!(
        label_space("did:plc:g", post).as_deref(),
        Some("at://did:plc:g/space/x.y/rides")
    );
    assert_eq!(label_space("did:plc:other", post), None);
    assert_eq!(label_space("did:plc:g", "at://did:plc:g/x.y/rides"), None);
    assert_eq!(label_space("did:plc:g", "https://example.com"), None);

    // The record, at a version if named, and which are the same label.
    let record = label_record(post, Some("bafy"), "spam", NOW);
    assert_eq!(
        record,
        json!({"$type": LABEL, "uri": post, "cid": "bafy", "val": "spam", "cts": NOW})
    );
    assert!(label_record(post, None, "spam", NOW).get("cid").is_none());
    assert!(labels(&record, post, "spam"));
    assert!(!labels(&record, post, "!hide"));
    assert!(!labels(&record, "did:plc:a", "spam"));
}

#[test]
fn plans_a_method_asked_with_its_arguments() {
    assert_eq!(
        planned("group_show", json!({})),
        Ok(Plan::Call {
            nsid: "org.atgrp.internal.getGroup".into(),
            procedure: false,
            input: json!({}),
        })
    );
    // Its opposite, with undo, which takes nothing but the group.
    let off = |undo: bool| planned("group_deactivate", json!({"undo": undo}));
    let call = |nsid: &str| Plan::Call {
        nsid: format!("org.atgrp.internal.{nsid}"),
        procedure: true,
        input: json!({}),
    };
    assert_eq!(off(false), Ok(call("deactivateGroup")));
    assert_eq!(off(true), Ok(call("activateGroup")));
    // The group served at its own domain, or with undo at its address.
    let domain = |undo: bool| planned("group_domain", json!({"undo": undo}));
    assert_eq!(domain(false), Ok(call("putGroupDomain")));
    assert_eq!(domain(true), Ok(call("deleteGroupDomain")));
    let Ok(Plan::Call { input, .. }) = planned("join_list", json!({"limit": 5})) else {
        panic!()
    };
    assert_eq!(input, json!({"limit": 5}));
    // A label defined, its flags in snake case asked in the method's own names.
    let args = json!({"label": "troll", "name": "Troll", "description": "Picks fights.", "default_setting": "hide", "adult_only": true});
    let Ok(Plan::Call { nsid, input, .. }) = planned("label_define", args) else {
        panic!()
    };
    assert_eq!(nsid, "org.atgrp.internal.putLabelDefinition");
    assert_eq!(
        input,
        json!({"label": "troll", "name": "Troll", "description": "Picks fights.", "defaultSetting": "hide", "adultOnly": true})
    );
    let Ok(Plan::Call { nsid, .. }) = planned("label_delete", json!({"label": "troll"})) else {
        panic!()
    };
    assert_eq!(nsid, "org.atgrp.internal.deleteLabelDefinition");
    // A subject and its version, sent as they are given.
    let call = |args: Value| match planned("mod_resolve", args).unwrap() {
        Plan::Call { nsid, input, .. } => (nsid, input),
        other => panic!("{other:?}"),
    };
    let post = "at://did:plc:g/space/x/self/did:plc:a/com.example.post/1";
    let asked = json!({"subject": post, "cid": "bafy", "status": "resolved"});
    assert_eq!(
        call(asked.clone()),
        ("org.atgrp.resolveModSubject".into(), asked)
    );
}

#[test]
fn plans_a_record_written_as_the_group() {
    let write = |space: &str, kind: &str, rkey: Option<&str>, record: Option<Value>| Plan::Write {
        space: format!("org.atgrp.{space}"),
        collection: format!("org.atgrp.{kind}"),
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
            space: "org.atgrp.meta".into(),
            collection: "org.atgrp.profile".into(),
            rkey: "self".into(),
            fields: json!({"displayName": "Riders", "description": ""})
                .as_object()
                .unwrap()
                .clone(),
        })
    );
    let rule = json!({"$type": "org.atgrp.rule", "title": "Be kind", "text": ""});
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
            Some(json!({"$type": "org.atgrp.role", "displayName": "Ride leader"}))
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
            Some(json!({"$type": "org.atgrp.membership", "roles": ["member"], "createdAt": NOW}))
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
                json!({"$type": "org.atgrp.invite", "message": "Ride with us", "expiresAt": "2099-01-01T00:00:00Z", "createdAt": NOW})
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
fn plans_a_space_under_its_policy() {
    let grants = json!([{"role": "member", "read": true}]);
    assert_eq!(
        planned(
            "space_create",
            json!({"type": "com.example.calendar", "skey": "rides", "policy": {"grants": grants}})
        ),
        Ok(Plan::Space {
            kind: "com.example.calendar".into(),
            skey: Some("rides".into()),
            policy: json!({"$type": "org.atgrp.policy", "grants": grants}),
        })
    );
    assert_eq!(
        planned("space_create", json!({"type": "com.example.calendar"})),
        Err("policy is required, as an object".into())
    );
}

#[test]
fn regrants_one_role_keeping_the_rest_of_its_record() {
    let written = |plan: Result<Plan, String>| match plan.unwrap() {
        Plan::Write {
            space,
            collection,
            rkey,
            record,
        } => {
            assert_eq!(
                (space.as_str(), collection.as_str(), rkey.as_deref()),
                ("org.atgrp.members", "org.atgrp.role", Some("leader"))
            );
            record.unwrap()
        }
        other => panic!("{other:?}"),
    };
    let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let kept = json!({"$type": "org.atgrp.role", "displayName": "Leader", "scopes": ["b", "c", "b"], "assignable": ["member"], "createdAt": NOW});
    // What is named is added to what the role had, once; the rest of the
    // record as it was.
    let record = written(regrant(
        &kept,
        "leader",
        &names(&["d", "b"]),
        &names(&["*"]),
        false,
    ));
    assert_eq!(
        record,
        json!({"$type": "org.atgrp.role", "displayName": "Leader", "scopes": ["b", "c", "d"], "assignable": ["member", "*"], "createdAt": NOW})
    );
    // With undo, what is named is taken away, a list left empty with it;
    // with nothing named, both lists go.
    let record = written(regrant(
        &kept,
        "leader",
        &names(&["b"]),
        &names(&["member"]),
        true,
    ));
    assert_eq!(record["scopes"], json!(["c"]));
    assert!(record.get("assignable").is_none());
    let record = written(regrant(&kept, "leader", &[], &[], true));
    assert_eq!(
        record,
        json!({"$type": "org.atgrp.role", "displayName": "Leader", "createdAt": NOW})
    );
    // A role the group does not have is not granted anything.
    assert_eq!(
        regrant(&Value::Null, "mod", &names(&["e"]), &[], false),
        Err("no such role: mod".into())
    );
}

#[test]
fn edits_a_record_keeping_what_is_not_given() {
    let avatar =
        json!({"$type": "blob", "ref": {"$link": "bafy"}, "mimeType": "image/png", "size": 3});
    let kept = json!({"$type": "org.atgrp.profile", "displayName": "Riders", "description": "Bikes", "avatar": avatar, "url": "https://a.test", "joinPolicy": "open"});
    let fields =
        json!({"displayName": "Riders United", "description": "", "url": null, "banner": avatar});
    let plan = edited(
        &kept,
        "org.atgrp.meta".into(),
        "org.atgrp.profile".into(),
        "self".into(),
        fields.as_object().unwrap().clone(),
    );
    assert_eq!(
        plan,
        Plan::Write {
            space: "org.atgrp.meta".into(),
            collection: "org.atgrp.profile".into(),
            rkey: Some("self".into()),
            record: Some(
                json!({"$type": "org.atgrp.profile", "displayName": "Riders United", "avatar": avatar, "banner": avatar, "joinPolicy": "open"})
            ),
        }
    );
    // None yet: only what is given.
    let fields = json!({"displayName": "Riders"});
    let plan = edited(
        &Value::Null,
        "org.atgrp.meta".into(),
        "org.atgrp.profile".into(),
        "self".into(),
        fields.as_object().unwrap().clone(),
    );
    let Plan::Write { record, .. } = plan else {
        panic!()
    };
    assert_eq!(
        record,
        Some(json!({"$type": "org.atgrp.profile", "displayName": "Riders"}))
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
