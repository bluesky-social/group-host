//! An MCP server for a group on an opensocial host. Its tools are the
//! core's methods, asked as the member who connected it, so an assistant
//! may do only what that member's roles allow: read the group, and run its
//! members, roles, invites and moderation.
//!
//! This crate is the protocol and the tools: what an assistant may call,
//! made from the methods' lexicons, and the answer to each message it
//! sends (JSON-RPC, over MCP's Streamable HTTP, each answered on its own).
//! The host serves it at `/mcp`, signs assistants in, and makes each call:
//! as the member, in the one group they connected the assistant to, which
//! is why no tool asks which group.

use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The versions of MCP this server speaks, the latest first.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What the server tells an assistant of itself as it connects.
const INSTRUCTIONS: &str = "The tools act as the person who connected you, in the one group \
they chose, with what their roles there allow: read the group, and run its members, roles, \
rules, invites, spaces and moderation. Anything their roles do not allow is refused.";

macro_rules! lexicon {
    ($name:literal) => {
        (
            concat!("fyi.opensocial.", $name),
            include_str!(concat!("../../../lexicons/fyi/opensocial/", $name, ".json")),
        )
    };
}

/// The methods an assistant may call, by their lexicons.
const METHODS: &[(&str, &str)] = &[
    lexicon!("getGroup"),
    lexicon!("updateProfile"),
    lexicon!("putRule"),
    lexicon!("deleteRule"),
    lexicon!("putRole"),
    lexicon!("deleteRole"),
    lexicon!("assignRoles"),
    lexicon!("listJoinRequests"),
    lexicon!("admitMember"),
    lexicon!("denyJoinRequest"),
    lexicon!("listInvites"),
    lexicon!("createInvite"),
    lexicon!("revokeInvite"),
    lexicon!("ejectMember"),
    lexicon!("createSpace"),
    lexicon!("listSpaceRecords"),
    lexicon!("listSubjects"),
    lexicon!("getSubjectHistory"),
    lexicon!("applyLabel"),
    lexicon!("negateLabel"),
    lexicon!("resolveSubject"),
];

/// What the methods refer to: ours, and atproto's.
const REFERRED: &[(&str, &str)] = &[
    lexicon!("defs"),
    lexicon!("access"),
    (
        "com.atproto.admin.defs",
        include_str!("../../../lexicons/upstream/com/atproto/admin/defs.json"),
    ),
    (
        "com.atproto.repo.strongRef",
        include_str!("../../../lexicons/upstream/com/atproto/repo/strongRef.json"),
    ),
];

/// A tool: a method of the host's, asked in the group the assistant was
/// connected to.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// Its name, the method's: `getGroup`.
    pub name: String,
    /// The method, `fyi.opensocial.getGroup`.
    pub nsid: String,
    /// Whether it is a procedure (a POST of JSON), not a query.
    pub procedure: bool,
    pub description: String,
    /// What it takes, as JSON Schema: the method's parameters or input,
    /// but the group.
    pub input_schema: Value,
}

/// The lexicons by NSID, to resolve references in.
struct Docs(HashMap<String, Value>);

/// `from`'s fields, as `(theirs, ours)`, into `to`.
fn copy(from: &Value, to: &mut Value, fields: &[(&str, &str)]) {
    for (theirs, ours) in fields {
        if let Some(v) = from.get(*theirs) {
            to[*ours] = v.clone();
        }
    }
}

impl Docs {
    fn load() -> Docs {
        let all = METHODS.iter().chain(REFERRED);
        let parsed = all.map(|(nsid, text)| {
            let doc = serde_json::from_str(text).expect("a lexicon is JSON");
            (nsid.to_string(), doc)
        });
        Docs(parsed.collect())
    }

    /// The def `reference` names, from within `doc`: `#name`, `nsid#name`,
    /// or `nsid` for its main; with the NSID of the document it is in.
    fn resolve(&self, doc: &str, reference: &str) -> Option<(String, &Value)> {
        let (nsid, name) = match reference.split_once('#') {
            Some(("", name)) => (doc, name),
            Some((nsid, name)) => (nsid, name),
            None => (reference, "main"),
        };
        let def = self.0.get(nsid)?.get("defs")?.get(name)?;
        Some((nsid.to_string(), def))
    }

    /// `def`, a lexicon type in `doc`, as JSON Schema; references followed
    /// `depth` deep, and one past that, or not among these lexicons, left
    /// open.
    fn schema(&self, doc: &str, def: &Value, depth: u8) -> Value {
        let mut out = match def["type"].as_str().unwrap_or_default() {
            "string" => {
                let mut s = json!({"type": "string"});
                copy(
                    def,
                    &mut s,
                    &[
                        ("maxLength", "maxLength"),
                        ("format", "format"),
                        ("enum", "enum"),
                    ],
                );
                if let Some(known) = def["knownValues"].as_array() {
                    let known: Vec<&str> = known.iter().filter_map(Value::as_str).collect();
                    s["description"] = json!(format!("One of: {}.", known.join(", ")));
                }
                s
            }
            "integer" => {
                let mut s = json!({"type": "integer"});
                copy(
                    def,
                    &mut s,
                    &[("minimum", "minimum"), ("maximum", "maximum")],
                );
                s
            }
            "boolean" => json!({"type": "boolean"}),
            "array" => {
                let items = self.schema(doc, &def["items"], depth);
                let mut s = json!({"type": "array", "items": items});
                copy(
                    def,
                    &mut s,
                    &[("maxLength", "maxItems"), ("minLength", "minItems")],
                );
                s
            }
            "object" | "params" => self.object(doc, def, depth),
            "ref" if depth > 0 => {
                let reference = def["ref"].as_str().unwrap_or_default();
                match self.resolve(doc, reference) {
                    Some((at, found)) => self.schema(&at, found, depth - 1),
                    None => json!({}),
                }
            }
            "union" if depth > 0 => {
                let refs = def["refs"].as_array().cloned().unwrap_or_default();
                let any: Vec<Value> = refs.iter().map(|r| self.member(doc, r, depth)).collect();
                json!({ "anyOf": any })
            }
            _ => json!({}),
        };
        if let Some(d) = def["description"].as_str() {
            let known = out["description"].as_str().map(|k| format!(" {k}"));
            out["description"] = json!(format!("{d}{}", known.unwrap_or_default()));
        }
        out
    }

    /// A member of a union, `reference`, which says which it is by its
    /// `$type`, as atproto has it.
    fn member(&self, doc: &str, reference: &Value, depth: u8) -> Value {
        let one = json!({"type": "ref", "ref": reference});
        let mut s = self.schema(doc, &one, depth);
        let name = reference.as_str().unwrap_or_default();
        // A local reference, `#name`, is named in full: `<doc>#name`.
        let local = if name.starts_with('#') { doc } else { "" };
        let name = format!("{local}{name}");
        if let Some(props) = s.get_mut("properties").and_then(Value::as_object_mut) {
            props.insert("$type".into(), json!({ "const": name }));
        }
        s
    }

    fn object(&self, doc: &str, def: &Value, depth: u8) -> Value {
        let properties: Map<String, Value> = def["properties"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), self.schema(doc, v, depth)))
            .collect();
        let required = def["required"].as_array().cloned().unwrap_or_default();
        json!({"type": "object", "properties": properties, "required": required})
    }
}

/// How deep the tools' schemas follow references.
const DEPTH: u8 = 6;

/// The tools, each a method, its schema without the group.
pub fn tools() -> Vec<Tool> {
    let docs = Docs::load();
    let tool = |nsid: &str| {
        let main = &docs.0[nsid]["defs"]["main"];
        let procedure = main["type"] == "procedure";
        let taken = match procedure {
            true => &main["input"]["schema"],
            false => &main["parameters"],
        };
        let mut input = docs.object(nsid, taken, DEPTH);
        let properties = input["properties"].as_object_mut();
        properties
            .expect("an object's schema has properties")
            .remove("group");
        let required = input["required"].as_array_mut();
        let required = required.expect("an object's schema says what it requires");
        required.retain(|r| r != "group");
        Tool {
            name: nsid.rsplit('.').next().unwrap_or(nsid).to_string(),
            nsid: nsid.to_string(),
            procedure,
            description: main["description"].as_str().unwrap_or_default().into(),
            input_schema: input,
        }
    };
    METHODS.iter().map(|(nsid, _)| tool(nsid)).collect()
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// The answer to a message that is not JSON at all: JSON-RPC's parse
/// error, with no id, as none could be read.
pub fn not_json() -> Value {
    error(&Value::Null, -32700, "a JSON-RPC message is JSON")
}

/// The answer to `message`, an assistant's JSON-RPC message, or none for
/// a notification: each tool called through `call`, with its arguments,
/// which answers what the method did, or why it did not.
pub async fn answer<F, Fut>(message: &Value, tools: &[Tool], call: F) -> Option<Value>
where
    F: FnOnce(Tool, Value) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    if !message.is_object() || message["jsonrpc"] != "2.0" {
        let said = "a JSON-RPC 2.0 message is an object";
        return Some(error(&Value::Null, -32600, said));
    }
    // A notification, with no id, is answered by nothing.
    let id = message.get("id")?;
    let params = &message["params"];
    let answered = match message["method"].as_str().unwrap_or_default() {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let known = PROTOCOL_VERSIONS.into_iter().find(|v| *v == asked);
            json!({
                "protocolVersion": known.unwrap_or(PROTOCOL_VERSIONS[0]),
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "group-host", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => {
            let listed = tools.iter().map(|t| {
                json!({"name": t.name, "description": t.description, "inputSchema": t.input_schema})
            });
            json!({ "tools": listed.collect::<Vec<_>>() })
        }
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            let Some(tool) = tools.iter().find(|t| t.name == name) else {
                return Some(error(id, -32602, &format!("no tool {name}")));
            };
            let arguments = match &params["arguments"] {
                Value::Null => json!({}),
                given @ Value::Object(_) => given.clone(),
                _ => return Some(error(id, -32602, "a tool's arguments are an object")),
            };
            match call(tool.clone(), arguments).await {
                Ok(done) => {
                    let text = done.to_string();
                    let mut out = json!({"content": [{"type": "text", "text": text}]});
                    if done.is_object() {
                        out["structuredContent"] = done;
                    }
                    out
                }
                Err(why) => json!({"content": [{"type": "text", "text": why}], "isError": true}),
            }
        }
        other => return Some(error(id, -32601, &format!("no method {other}"))),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": answered}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A union of a lexicon's own defs: each member names itself in full.
    #[test]
    fn names_a_local_member_of_a_union_in_full() {
        let doc =
            json!({"defs": {"a": {"type": "object", "properties": {"x": {"type": "string"}}}}});
        let docs = Docs(HashMap::from([("com.example.thing".to_string(), doc)]));
        let union = json!({"type": "union", "refs": ["#a"]});
        let schema = docs.schema("com.example.thing", &union, DEPTH);
        let member = &schema["anyOf"][0]["properties"]["$type"]["const"];
        assert_eq!(member, "com.example.thing#a");
    }
}
