//! An MCP server for a group on an opensocial host, and the tools it
//! offers: each the twin of a command of `opensocial`, named
//! `<group>_<action>` as the command is `opensocial <group> <action>`, and
//! taking what the command takes. Each is asked as the member who
//! connected the MCP client, so it may do only what that member's roles
//! allow: read the group, change its profile and rules, and run its
//! members, roles, invites, spaces and moderation.
//!
//! This crate is the protocol and the tools: what an MCP client may call, made
//! from the lexicons; what each call comes to (`plan`), which the host and
//! the command both carry out; and the answer to each message an MCP client
//! sends (JSON-RPC, over MCP's Streamable HTTP, each answered on its own).
//! The host serves it at `/mcp`, signs MCP clients in, and makes each call: as
//! the member, in the one group they connected the MCP client to, which is why
//! no tool asks which group.

use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The versions of MCP this server speaks, the latest first.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What the server tells an MCP client of itself as it connects.
const INSTRUCTIONS: &str = "The tools act as the person who connected you, in the one group \
they chose, with what their roles there allow: read the group, change its profile and rules, \
and run its members, roles, invites, spaces and moderation. Each is a command of `opensocial`, \
`<group> <action>`, named `<group>_<action>`. Anything their roles do not allow is refused.";

macro_rules! lexicon {
    ($name:literal) => {
        (
            concat!("fyi.opensocial.", $name),
            include_str!(concat!("../../../lexicons/fyi/opensocial/", $name, ".json")),
        )
    };
}

/// The lexicons the tools are made from, and what they refer to: ours,
/// and atproto's.
const LEXICONS: &[(&str, &str)] = &[
    lexicon!("getGroup"),
    lexicon!("deactivateGroup"),
    lexicon!("activateGroup"),
    lexicon!("listMembers"),
    lexicon!("listJoinRequests"),
    lexicon!("denyJoinRequest"),
    lexicon!("listInvites"),
    lexicon!("createSpace"),
    lexicon!("listSpaceRecords"),
    lexicon!("listSubjects"),
    lexicon!("getSubjectHistory"),
    lexicon!("applyLabel"),
    lexicon!("negateLabel"),
    lexicon!("resolveSubject"),
    lexicon!("putSite"),
    lexicon!("deleteSite"),
    lexicon!("profile"),
    lexicon!("rule"),
    lexicon!("role"),
    lexicon!("permissions"),
    lexicon!("membership"),
    lexicon!("defs"),
    lexicon!("access"),
    lexicon!("invite"),
    (
        "com.atproto.admin.defs",
        include_str!("../../../lexicons/upstream/com/atproto/admin/defs.json"),
    ),
    (
        "com.atproto.repo.strongRef",
        include_str!("../../../lexicons/upstream/com/atproto/repo/strongRef.json"),
    ),
];

const NS: &str = "fyi.opensocial";
/// The group's spaces a tool writes its records in, by their types.
const META: &str = "fyi.opensocial.meta";
const MEMBERS: &str = "fyi.opensocial.members";

/// Where a record a tool writes is kept in its space.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    /// At the key this argument names, which the tool requires.
    Named(String),
    /// At the key this field of the record names, which the tool
    /// requires: a membership's member.
    Field(String),
    /// At the key this argument names if given; else a new one.
    New(String),
}

/// What calling a tool does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Asks a method of the host's, a query or a procedure; with `undo`,
    /// its opposite. A method whose `subject` is a union of an account and
    /// a record takes it as text, a DID or an `at://` URI, with a `cid`.
    Method {
        nsid: String,
        procedure: bool,
        undo: Option<String>,
        subject: Option<[String; 2]>,
    },
    /// Puts a record of the group's, of the kind `collection`, in its
    /// space of the type `space`, replacing it whole; saying when it was
    /// made if its kind requires that.
    Put {
        collection: String,
        space: String,
        key: Key,
        stamp: bool,
    },
    /// Changes the fields given of the record of the kind `collection` at
    /// `rkey` in the group's space of the type `space`, keeping the rest.
    Edit {
        collection: String,
        space: String,
        rkey: String,
    },
    /// Deletes one.
    Delete {
        collection: String,
        space: String,
        key: Key,
    },
    /// Writes an invite as the group in the invitee's inbox; with `undo`,
    /// takes back the one waiting.
    Invite,
    /// Adds to a role's grant in the permissions record; with `undo`,
    /// takes away from it.
    Grant,
}

/// A tool, asked in the group the MCP client was connected to; and a
/// command, asked in the group named first.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// Its name, `<group>_<action>`: `member_assign`.
    pub name: String,
    /// The arguments its command takes in order, after the group, rather
    /// than as flags: `member`, `roles`.
    pub args: Vec<String>,
    pub action: Action,
    pub description: String,
    /// What it takes, as JSON Schema: its arguments and flags, named in
    /// snake case, made from the lexicons.
    pub input_schema: Value,
}

impl Tool {
    /// Its command's words: `member assign`.
    pub fn command(&self) -> String {
        self.name.replacen('_', " ", 1).replace('_', "-")
    }
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
        let all = LEXICONS.iter();
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

/// Where a record a tool writes is kept, as `TOOLS` says it.
enum At {
    /// At the key this argument names, which is not the record's own.
    Named(&'static str),
    /// At the key this field of the record names: a membership's member.
    Field(&'static str),
    /// At the key this argument names if given; else a new one.
    New(&'static str),
}

/// What a tool is made of, as `TOOLS` says it.
enum Made {
    /// A method of ours, by the last part of its NSID; and its opposite's.
    Method(&'static str, Option<&'static str>),
    /// A record of ours put, or deleted, in the group's space of a type.
    Put(&'static str, &'static str, At),
    /// A record of ours at its one key, changed in the fields given.
    Edit(&'static str, &'static str, &'static str),
    Delete(&'static str, &'static str, At),
    Invite,
    Grant,
}

/// The tools, by group: each its name, the arguments its command takes in
/// order after the group, what it does, and what it is made of.
const TOOLS: &[(&str, &[&str], &str, Made)] = &[
    (
        "group_show",
        &[],
        "The group as the member may see it.",
        Made::Method("getGroup", None),
    ),
    (
        "group_deactivate",
        &[],
        "Switches the group off: nobody can read its spaces or write as it until one who \
         could switch it off switches it back on. With undo, switches it back on.",
        Made::Method("deactivateGroup", Some("activateGroup")),
    ),
    (
        "profile_set",
        &[],
        "Changes the group's profile as the group. Changes only the fields given, keeping the \
         rest; a field given empty, or null, is taken away.",
        Made::Edit("profile", META, "self"),
    ),
    (
        "rule_set",
        &[],
        "Writes a rule as the group: the one `rule` names, replaced whole, or a new one.",
        Made::Put("rule", META, At::New("rule")),
    ),
    (
        "rule_delete",
        &["rule"],
        "Deletes a rule as the group.",
        Made::Delete("rule", META, At::Named("rule")),
    ),
    (
        "role_set",
        &["role"],
        "Writes a role as the group, at its id, replaced whole.",
        Made::Put("role", MEMBERS, At::Named("role")),
    ),
    (
        "role_delete",
        &["role"],
        "Deletes a role nobody holds or is granted, as the group.",
        Made::Delete("role", MEMBERS, At::Named("role")),
    ),
    (
        "role_grant",
        &["role"],
        "Adds to what a role may do, as the group: the scopes given, and the roles given that its \
         holders may assign, to what its grant in the permissions record has; with undo, takes \
         them away, or with none given drops its grant.",
        Made::Grant,
    ),
    (
        "site_publish",
        &[],
        "Publishes the group's website, whole, at its handle (`https://<handle>/`): every file \
         of it, each as text or as bytes, in place of the site there; a file the group holds \
         already is used again. Its pages run sandboxed, each in an origin of its own, with no \
         cookies or storage; a page reads live data from the host's public methods.",
        Made::Method("putSite", None),
    ),
    (
        "site_delete",
        &[],
        "Takes the group's website down.",
        Made::Method("deleteSite", None),
    ),
    (
        "member_list",
        &[],
        "Who is in the group, and their roles, a page at a time.",
        Made::Method("listMembers", None),
    ),
    (
        "member_assign",
        &["member", "roles"],
        "Gives a member these roles in place of theirs, as the group (`member` among them), or \
         lets in someone waiting to join.",
        Made::Put("membership", MEMBERS, At::Field("member")),
    ),
    (
        "member_eject",
        &["member"],
        "Puts a member out, as the group.",
        Made::Delete("membership", MEMBERS, At::Field("member")),
    ),
    (
        "join_list",
        &[],
        "The requests to join waiting.",
        Made::Method("listJoinRequests", None),
    ),
    (
        "join_deny",
        &["member"],
        "Turns down a request to join.",
        Made::Method("denyJoinRequest", None),
    ),
    (
        "invite_send",
        &["person"],
        "Invites someone not yet a member, as the group: an invite written in their own inbox \
         (fyi.opensocial.invites); one already waiting for them is the one answered. With undo, \
         takes back the invite waiting for them.",
        Made::Invite,
    ),
    (
        "invite_list",
        &[],
        "The invites waiting.",
        Made::Method("listInvites", None),
    ),
    (
        "space_create",
        &["type"],
        "Adds a space to the group.",
        Made::Method("createSpace", None),
    ),
    (
        "space_records",
        &["space"],
        "The records in one of the group's spaces.",
        Made::Method("listSpaceRecords", None),
    ),
    (
        "mod_list",
        &[],
        "The moderation queue.",
        Made::Method("listSubjects", None),
    ),
    (
        "mod_history",
        &["subject"],
        "What was done about a subject.",
        Made::Method("getSubjectHistory", None),
    ),
    (
        "mod_resolve",
        &["subject"],
        "Resolves or escalates a subject.",
        Made::Method("resolveSubject", None),
    ),
    (
        "mod_label",
        &["subject", "val"],
        "Labels a subject; with undo, takes the label back.",
        Made::Method("applyLabel", Some("negateLabel")),
    ),
];

/// `displayName` as `display_name`.
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// `display_name` as `displayName`.
pub fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
            continue;
        }
        out.push(if upper { c.to_ascii_uppercase() } else { c });
        upper = false;
    }
    out
}

/// The key a record tool is told of.
fn rkey(described: String) -> Value {
    json!({"type": "string", "format": "record-key", "description": described})
}

/// `schema`'s properties, without `name`; whether it required it.
fn drop(schema: &mut Value, name: &str) -> bool {
    let properties = schema["properties"].as_object_mut();
    properties.expect("an object has properties").remove(name);
    let required = schema["required"].as_array_mut();
    let required = required.expect("an object says what it requires");
    let had = required.iter().any(|r| r == name);
    required.retain(|r| r != name);
    had
}

/// `schema` with `name`, as `property`, required or not.
fn add(schema: &mut Value, name: &str, property: Value, required: bool) {
    schema["properties"][name] = property;
    if required {
        let list = schema["required"].as_array_mut();
        list.expect("an object says what it requires")
            .push(json!(name));
    }
}

/// `schema`, an object's, its properties named in snake case.
fn snaked(schema: &Value) -> Value {
    let properties: Map<String, Value> = schema["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (snake(k), v.clone()))
        .collect();
    let required = schema["required"].as_array().into_iter().flatten();
    let required: Vec<Value> = required
        .filter_map(Value::as_str)
        .map(|r| json!(snake(r)))
        .collect();
    json!({"type": "object", "properties": properties, "required": required})
}

impl Docs {
    fn main(&self, nsid: &str) -> &Value {
        &self.0[nsid]["defs"]["main"]
    }

    /// The tool `TOOLS` makes so.
    fn tool(&self, (name, args, what, made): &(&str, &[&str], &str, Made)) -> Tool {
        let (action, mut input, about) = match made {
            Made::Method(method, opposite) => {
                let nsid = format!("{NS}.{method}");
                let main = self.main(&nsid);
                let procedure = main["type"] == "procedure";
                let taken = match procedure {
                    true => &main["input"]["schema"],
                    false => &main["parameters"],
                };
                let mut input = self.object(&nsid, taken, DEPTH);
                drop(&mut input, "group");
                // A union of an account and a record, as text.
                let union = &taken["properties"]["subject"];
                let subject = (union["type"] == "union").then(|| {
                    let refs = union["refs"].as_array().expect("a union has refs");
                    let text = |i: usize| refs[i].as_str().expect("a ref").to_string();
                    [text(0), text(1)]
                });
                if subject.is_some() {
                    let said = "An account's DID, or a record's at:// URI.";
                    let text = json!({"type": "string", "description": said});
                    add(&mut input, "subject", text, true);
                    let said = "The record's version, its CID: one a label is about needs it.";
                    let cid = json!({"type": "string", "format": "cid", "description": said});
                    add(&mut input, "cid", cid, false);
                }
                let action = Action::Method {
                    nsid,
                    procedure,
                    undo: opposite.map(|o| format!("{NS}.{o}")),
                    subject,
                };
                (action, input, main["description"].as_str())
            }
            Made::Put(kind, space, at) | Made::Delete(kind, space, at) => {
                let collection = format!("{NS}.{kind}");
                let main = self.main(&collection);
                let whole = self.object(&collection, &main["record"], DEPTH);
                let delete = matches!(made, Made::Delete(..));
                let mut input = match delete {
                    true => json!({"type": "object", "properties": {}, "required": []}),
                    false => whole.clone(),
                };
                let stamp = drop(&mut input, "createdAt");
                let key = match at {
                    At::Named(n) => {
                        let said = format!("Which {kind}: its record's key.");
                        add(&mut input, n, rkey(said), true);
                        Key::Named(n.to_string())
                    }
                    At::Field(n) => {
                        drop(&mut input, n);
                        add(&mut input, n, whole["properties"][*n].clone(), true);
                        Key::Field(n.to_string())
                    }
                    At::New(n) => {
                        let said = format!(
                            "Which {kind} to change: its record's key. Left out, a new one."
                        );
                        add(&mut input, n, rkey(said), false);
                        Key::New(n.to_string())
                    }
                };
                let (collection, space) = (collection.clone(), space.to_string());
                let action = match delete {
                    true => Action::Delete {
                        collection,
                        space,
                        key,
                    },
                    false => Action::Put {
                        collection,
                        space,
                        key,
                        stamp,
                    },
                };
                (action, input, main["description"].as_str())
            }
            Made::Edit(kind, space, rkey) => {
                let collection = format!("{NS}.{kind}");
                let main = self.main(&collection);
                let mut input = self.object(&collection, &main["record"], DEPTH);
                drop(&mut input, "createdAt");
                input["required"] = json!([]);
                let action = Action::Edit {
                    collection,
                    space: space.to_string(),
                    rkey: rkey.to_string(),
                };
                (action, input, main["description"].as_str())
            }
            Made::Invite => {
                let nsid = format!("{NS}.invite");
                let main = self.main(&nsid);
                let mut input = self.object(&nsid, &main["record"], DEPTH);
                drop(&mut input, "createdAt");
                let said = "Who is invited, by their DID.";
                let person = json!({"type": "string", "format": "did", "description": said});
                add(&mut input, "person", person, true);
                (Action::Invite, input, main["description"].as_str())
            }
            Made::Grant => {
                let defs = format!("{NS}.defs");
                let grant = &self.0[&defs]["defs"]["groupGrant"];
                let mut input = self.object(&defs, grant, DEPTH);
                input["required"] = json!(["role"]);
                let main = self.main(&format!("{NS}.permissions"));
                (Action::Grant, input, main["description"].as_str())
            }
        };
        let undoes = matches!(
            action,
            Action::Invite | Action::Grant | Action::Method { undo: Some(_), .. }
        );
        if undoes {
            let said = "Do the opposite, as the tool's description says.";
            add(
                &mut input,
                "undo",
                json!({"type": "boolean", "description": said}),
                false,
            );
        }
        let input = snaked(&input);
        Tool {
            name: name.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            action,
            description: format!("{what} {}", about.unwrap_or_default()),
            input_schema: input,
        }
    }
}

/// The tools, by group.
pub fn tools() -> Vec<Tool> {
    let docs = Docs::load();
    TOOLS.iter().map(|t| docs.tool(t)).collect()
}

/// What calling a tool comes to, carried out by the host for an MCP client and
/// by the command for a person.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// Ask the method `nsid` with `input`, and the group, which the one
    /// asking adds.
    Call {
        nsid: String,
        procedure: bool,
        input: Value,
    },
    /// Write a record of the group's in its space of the type `space`: put
    /// `record` at `rkey`, or at a new key if none; with no record, delete
    /// the one at `rkey`.
    Write {
        space: String,
        collection: String,
        rkey: Option<String>,
        record: Option<Value>,
    },
    /// Write `record` as an invite in `person`'s inbox; with none, take
    /// back the one waiting for them.
    Invite {
        person: String,
        record: Option<Value>,
    },
    /// Change the record of the kind `collection` at `rkey` in the
    /// group's space of the type `space` in the `fields` given, keeping
    /// the rest. `edited` makes the write, from the record as it is.
    Edit {
        space: String,
        collection: String,
        rkey: String,
        fields: Map<String, Value>,
    },
    /// Add `scopes` and `assignable` to `role`'s grant in the permissions
    /// record; with `undo`, take them away, or with none drop its grant.
    /// `regrant` makes the write, from the record as it is.
    Grant {
        role: String,
        scopes: Vec<String>,
        assignable: Vec<String>,
        undo: bool,
    },
}

/// What calling `tool` with `args` comes to, `now` the time a record it
/// makes says it was made.
pub fn plan(tool: &Tool, args: &Value, now: &str) -> Result<Plan, String> {
    let mut input: Map<String, Value> = args
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (camel(k), v.clone()))
        .collect();
    let undo = input.remove("undo") == Some(Value::Bool(true));
    let given = |input: &mut Map<String, Value>, name: &str| match input.remove(&camel(name)) {
        Some(Value::String(s)) => Ok(s),
        _ => Err(format!("{name} is required, as text")),
    };
    let stamped = |mut record: Map<String, Value>, collection: &str| {
        record.insert("$type".into(), json!(collection));
        record.insert("createdAt".into(), json!(now));
        Value::Object(record)
    };
    Ok(match &tool.action {
        Action::Method {
            nsid,
            procedure,
            undo: opposite,
            subject,
        } => {
            if let (Some([account, record]), Some(Value::String(s))) =
                (subject, input.get("subject"))
            {
                let made = match s.starts_with("did:") {
                    true => json!({"$type": account, "did": s}),
                    false => {
                        let mut made = json!({"$type": record, "uri": s});
                        if let Some(cid) = input.remove("cid") {
                            made["cid"] = cid;
                        }
                        made
                    }
                };
                input.insert("subject".into(), made);
            }
            let nsid = match (undo, opposite) {
                (true, Some(opposite)) => opposite,
                _ => nsid,
            };
            Plan::Call {
                nsid: nsid.clone(),
                procedure: *procedure,
                input: Value::Object(input),
            }
        }
        Action::Put {
            collection,
            space,
            key,
            ..
        }
        | Action::Delete {
            collection,
            space,
            key,
        } => {
            let rkey = match key {
                Key::Named(n) => Some(given(&mut input, n)?),
                Key::Field(n) => {
                    let named = given(&mut input, n)?;
                    input.insert(n.clone(), json!(named));
                    Some(named)
                }
                Key::New(n) => given(&mut input, n).ok(),
            };
            let record = match &tool.action {
                Action::Put { stamp: true, .. } => Some(stamped(input, collection)),
                Action::Put { .. } => {
                    input.insert("$type".into(), json!(collection));
                    Some(Value::Object(input))
                }
                _ => None,
            };
            Plan::Write {
                space: space.clone(),
                collection: collection.clone(),
                rkey,
                record,
            }
        }
        Action::Edit {
            collection,
            space,
            rkey,
        } => Plan::Edit {
            space: space.clone(),
            collection: collection.clone(),
            rkey: rkey.clone(),
            fields: input,
        },
        Action::Invite => {
            let person = given(&mut input, "person")?;
            let record = (!undo).then(|| stamped(input, &format!("{NS}.invite")));
            Plan::Invite { person, record }
        }
        Action::Grant => {
            let role = given(&mut input, "role")?;
            let list = |name: &str| -> Vec<String> {
                let items = input.get(name).and_then(Value::as_array).into_iter();
                let items = items.flatten().filter_map(Value::as_str);
                items.map(String::from).collect()
            };
            Plan::Grant {
                role,
                scopes: list("scopes"),
                assignable: list("assignable"),
                undo,
            }
        }
    })
}

/// The write that adds `scopes` and `assignable` to `role`'s grant, or
/// with `undo` takes them away, in `permissions`, the group's permissions
/// record as it is (none if it has none): the grant dropped if nothing is
/// named to take away, or nothing is left; each other role's as it was.
pub fn regrant(
    permissions: &Value,
    role: &str,
    scopes: &[String],
    assignable: &[String],
    undo: bool,
) -> Plan {
    let mut record = match permissions {
        Value::Object(_) => permissions.clone(),
        _ => json!({}),
    };
    let old = record["grants"].as_array().cloned().unwrap_or_default();
    let had = |field: &str| -> Vec<String> {
        let theirs = old.iter().filter(|g| g["role"] == role);
        let items = theirs.flat_map(|g| g[field].as_array().cloned().unwrap_or_default());
        let mut out: Vec<String> = Vec::new();
        for item in items.filter_map(|i| i.as_str().map(String::from)) {
            if !out.contains(&item) {
                out.push(item);
            }
        }
        out
    };
    let changed = |mut list: Vec<String>, named: &[String]| {
        match undo {
            true => list.retain(|i| !named.contains(i)),
            false => list.extend(
                named
                    .iter()
                    .filter(|n| !list.contains(n))
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
        }
        list
    };
    let scopes_now = changed(had("scopes"), scopes);
    let assignable_now = changed(had("assignable"), assignable);
    let named = !scopes.is_empty() || !assignable.is_empty();
    let gone = undo && (!named || (scopes_now.is_empty() && assignable_now.is_empty()));
    let grant =
        (!gone).then(|| json!({"role": role, "scopes": scopes_now, "assignable": assignable_now}));
    let mut grants = Vec::new();
    let mut placed = false;
    for g in old {
        match g["role"] == role {
            true if !placed => {
                placed = true;
                grants.extend(grant.clone());
            }
            true => {}
            false => grants.push(g),
        }
    }
    if !placed {
        grants.extend(grant);
    }
    record["$type"] = json!(format!("{NS}.permissions"));
    record["grants"] = json!(grants);
    Plan::Write {
        space: MEMBERS.into(),
        collection: format!("{NS}.permissions"),
        rkey: Some("self".into()),
        record: Some(record),
    }
}

/// The write that changes `kept`, the record of the kind `collection` at
/// `rkey` in the group's space of the type `space` as it is (none if there
/// is none), in the `fields` given: each set, or taken away if given
/// empty, an empty list, or null; the rest as they were.
pub fn edited(
    kept: &Value,
    space: String,
    collection: String,
    rkey: String,
    fields: Map<String, Value>,
) -> Plan {
    let mut record = match kept {
        Value::Object(o) => o.clone(),
        _ => Map::new(),
    };
    for (name, value) in fields {
        let empty = match &value {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(a) => a.is_empty(),
            _ => false,
        };
        match empty {
            true => record.remove(&name),
            false => record.insert(name, value),
        };
    }
    record.insert("$type".into(), json!(collection));
    Plan::Write {
        space,
        collection,
        rkey: Some(rkey),
        record: Some(Value::Object(record)),
    }
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// The answer to a message that is not JSON at all: JSON-RPC's parse
/// error, with no id, as none could be read.
pub fn not_json() -> Value {
    error(&Value::Null, -32700, "a JSON-RPC message is JSON")
}

/// The answer to `message`, an MCP client's JSON-RPC message, or none for
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
