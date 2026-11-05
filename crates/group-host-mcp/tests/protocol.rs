//! The tools, as an assistant lists them, and what the server answers
//! each message it sends.

use group_host_mcp::{Kind, PROTOCOL_VERSIONS, Tool, answer, tools};
use serde_json::{Value, json};

/// What the server answers `message`, each tool answering as `said` says.
async fn ask(message: Value, said: Result<Value, String>) -> Option<Value> {
    let tools = tools();
    answer(&message, &tools, |_: Tool, _: Value| async move { said }).await
}

fn tool(name: &str) -> Tool {
    tools().into_iter().find(|t| t.name == name).unwrap()
}

#[test]
fn makes_a_tool_of_each_method_without_its_group() {
    let all = tools();
    assert!(all.len() > 15);
    for t in &all {
        let record = matches!(t.kind, Kind::Record { .. });
        let method = t.nsid.ends_with(&t.name);
        assert!(
            method || record || t.nsid.starts_with("com.atproto.space."),
            "{t:?}"
        );
        assert!(
            t.input_schema["properties"].get("group").is_none(),
            "{}",
            t.name
        );
        let required = t.input_schema["required"].as_array();
        assert!(
            !required.into_iter().flatten().any(|r| r == "group"),
            "{}",
            t.name
        );
        assert!(!t.description.is_empty(), "{}", t.name);
    }
    let get = tool("getGroup");
    assert_eq!(get.kind, Kind::Query);
    assert_eq!(
        get.input_schema,
        json!({"type": "object", "properties": {}, "required": []})
    );

    // A membership, written as the group in its members space, at its
    // member's DID; a reference followed: each role its id.
    let admit = tool("putMembership");
    let members = "fyi.opensocial.members".to_string();
    let put = Kind::Record {
        space: members.clone(),
        literal: None,
        delete: false,
    };
    assert_eq!(admit.kind, put);
    let fields = &admit.input_schema["properties"];
    assert_eq!(fields["member"]["format"], "did");
    assert_eq!(fields["roles"]["items"]["type"], "string");
    assert_eq!(fields["rkey"]["format"], "record-key");
    let gone = Kind::Record {
        space: members,
        literal: None,
        delete: true,
    };
    assert_eq!(tool("deleteMembership").kind, gone);
    assert_eq!(tool("deleteRole").kind, gone);
    // The permissions record, at self; its grants' scopes plain strings.
    let grants = &tool("putPermissions").input_schema["properties"]["grants"];
    assert_eq!(
        grants["items"]["properties"]["scopes"]["items"]["type"],
        "string"
    );
    assert!(tools().iter().all(|t| t.name != "deletePermissions"));

    // Known values said in words, beside what the lexicon says; a closed
    // set, as JSON Schema's own.
    let policy = &tool("putProfile").input_schema["properties"]["joinPolicy"];
    let said = policy["description"].as_str().unwrap();
    assert!(said.contains("One of: "), "{said}");
    let status = &tool("listSubjects").input_schema["properties"]["status"];
    assert_eq!(status["enum"], json!(["open", "escalated", "resolved"]));
    let limit = &tool("listSubjects").input_schema["properties"]["limit"];
    assert_eq!(limit["type"], "integer");
    assert!(limit.get("maximum").is_some());

    // A union, each member saying which it is.
    let subject = &tool("applyLabel").input_schema["properties"]["subject"];
    let members = subject["anyOf"].as_array().unwrap();
    assert!(members.len() >= 2, "{subject}");
    assert!(
        members
            .iter()
            .filter_map(|m| m["properties"]["$type"]["const"].as_str())
            .count()
            >= 1,
        "{subject}"
    );
    let online = &tool("putProfile").input_schema["properties"]["online"];
    assert_eq!(online["type"], "boolean");

    // Invites, written as the group: the record's fields but when it was
    // made, which the host says, and whom it is for.
    let send = tool("sendInvite");
    assert_eq!(
        (send.nsid.as_str(), send.kind),
        ("com.atproto.space.createRecord", Kind::Procedure)
    );
    let fields = send.input_schema["properties"].as_object().unwrap();
    let mut names: Vec<&str> = fields.keys().map(String::as_str).collect();
    names.sort();
    assert_eq!(names, ["expiresAt", "invitee", "message"]);
    assert_eq!(send.input_schema["required"], json!(["invitee"]));
    assert_eq!(fields["message"]["maxLength"], 20000);
    let take_back = tool("takeBackInvite");
    assert_eq!(take_back.nsid, "com.atproto.space.deleteRecord");
    assert_eq!(take_back.input_schema["required"], json!(["invitee"]));
}

#[test]
fn makes_a_tool_of_each_record_written_as_the_group() {
    // The profile, kept at one key, as its lexicon has it: no key to say.
    let profile = tool("putProfile");
    assert_eq!(profile.nsid, "fyi.opensocial.profile");
    let meta = "fyi.opensocial.meta".to_string();
    assert_eq!(
        profile.kind,
        Kind::Record {
            space: meta.clone(),
            literal: Some("self".into()),
            delete: false
        }
    );
    let fields = &profile.input_schema["properties"];
    assert!(fields.get("rkey").is_none(), "{fields}");
    assert_eq!(fields["displayName"]["maxLength"], 640);
    assert_eq!(
        profile.input_schema["required"],
        json!(["displayName", "joinPolicy"])
    );
    assert!(profile.description.contains("The group's profile"));

    // A rule, at a key of its own: one to change, or none for a new one.
    let rule = tool("putRule");
    assert_eq!(rule.nsid, "fyi.opensocial.rule");
    let fields = &rule.input_schema["properties"];
    assert_eq!(fields["rkey"]["type"], "string");
    assert_eq!(fields["title"]["type"], "string");
    assert!(
        !rule.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "rkey")
    );
    let delete = tool("deleteRule");
    assert_eq!(
        delete.kind,
        Kind::Record {
            space: meta,
            literal: None,
            delete: true
        }
    );
    assert_eq!(delete.input_schema["required"], json!(["rkey"]));
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
    let call = |args: Value| json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "getGroup", "arguments": args}});
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
        Ok(json!({"tool": t.nsid, "args": args}))
    })
    .await
    .unwrap();
    assert_eq!(
        said["result"]["structuredContent"],
        json!({"tool": "fyi.opensocial.getGroup", "args": {"x": 1}})
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
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "getGroup", "arguments": [1]}}),
                Ok(json!({}))
            )
            .await
        ),
        -32602
    );
}
