//! An MCP server for a group on an Atmosphere Groups host, and the tools it
//! offers: each the twin of a command of `atgrp`, named
//! `<group>_<action>` as the command is `atgrp <group> <action>`, and
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
and run its members, roles, invites, spaces and moderation. Each is a command of `atgrp`, \
`<group> <action>`, named `<group>_<action>`. Anything their roles do not allow is refused.";

/// A lexicon of ours: in the proposal, by its name; or the host's own,
/// `internal` and its name, as `org.atgrp.internal.<name>`.
macro_rules! lexicon {
    ($name:literal) => {
        (
            concat!("org.atgrp.", $name),
            include_str!(concat!("../../../lexicons/org/atgrp/", $name, ".json")),
        )
    };
    (mod $name:literal) => {
        (
            concat!("org.atgrp.mod.", $name),
            include_str!(concat!("../../../lexicons/org/atgrp/mod/", $name, ".json")),
        )
    };
    (internal $name:literal) => {
        (
            concat!("org.atgrp.internal.", $name),
            include_str!(concat!(
                "../../../lexicons/org/atgrp/internal/",
                $name,
                ".json"
            )),
        )
    };
}

/// The lexicons the tools are made from, and what they refer to: ours,
/// and atproto's.
const LEXICONS: &[(&str, &str)] = &[
    lexicon!(internal "getGroup"),
    lexicon!(internal "deactivateGroup"),
    lexicon!(internal "activateGroup"),
    lexicon!(internal "updateGroupHandle"),
    lexicon!(internal "putGroupDomain"),
    lexicon!(internal "deleteGroupDomain"),
    lexicon!(internal "listMembers"),
    lexicon!("listJoinRequests"),
    lexicon!("denyJoinRequest"),
    lexicon!(internal "listInvites"),
    lexicon!(internal "listSpaceRecords"),
    lexicon!(mod "querySubjects"),
    lexicon!(mod "querySubjectHistory"),
    lexicon!(internal "putLabelDefinition"),
    lexicon!(internal "deleteLabelDefinition"),
    lexicon!(mod "updateSubject"),
    lexicon!(mod "defs"),
    lexicon!("about"),
    lexicon!("config"),
    lexicon!("rule"),
    lexicon!("role"),
    lexicon!("membership"),
    lexicon!("defs"),
    lexicon!(internal "defs"),
    lexicon!("policy"),
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

const NS: &str = "org.atgrp";
/// The group's spaces a tool writes its records in, by their types.
const META: &str = "org.atgrp.meta";
const MEMBERS: &str = "org.atgrp.members";

/// Where a record a tool writes is kept in its space.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    /// At the key this argument names, which the tool requires.
    Named(String),
    /// At the key this argument names if given; else a new one.
    New(String),
}

/// What calling a tool does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Asks a method of the host's, a query or a procedure; with `undo`,
    /// its opposite.
    Method {
        nsid: String,
        procedure: bool,
        undo: Option<String>,
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
    /// Changes the fields given of the records at `rkey` in the group's
    /// space of the type `space`, keeping the rest: each record, by its
    /// collection, with the fields it has; only those with a field given.
    Edit {
        records: Vec<(String, Vec<String>)>,
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
    /// Adds to a role's permissions and the roles it may assign, in its record;
    /// with `undo`, takes away from them.
    Grant,
    /// Writes a label as the group beside its subject; with `undo`,
    /// deletes the labels of that value there.
    Label,
    /// Makes a space of the type `type` under the group, with the policy
    /// given, as the group (`com.atproto.space.createSpace`).
    Space,
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

    /// The def `reference` names, from within `doc`: `#name` or
    /// `nsid#name`; with the NSID of the document it is in.
    fn resolve(&self, doc: &str, reference: &str) -> Option<(String, &Value)> {
        let (nsid, name) = reference.split_once('#')?;
        let nsid = if nsid.is_empty() { doc } else { nsid };
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
            "ref"
                if depth > 0
                    && let Some((at, found)) =
                        self.resolve(doc, def["ref"].as_str().unwrap_or_default()) =>
            {
                self.schema(&at, found, depth - 1)
            }
            _ => json!({}),
        };
        if let Some(d) = def["description"].as_str() {
            let known = out["description"].as_str().map(|k| format!(" {k}"));
            out["description"] = json!(format!("{d}{}", known.unwrap_or_default()));
        }
        out
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
    /// At the key this argument, a member's DID, names: a membership's.
    Did(&'static str),
    /// At the key this argument names if given; else a new one.
    New(&'static str),
}

/// What a tool is made of, as `TOOLS` says it.
enum Made {
    /// A method of ours, by its NSID past `org.atgrp.` (the host's
    /// own `internal.<name>`); and its opposite's.
    Method(&'static str, Option<&'static str>),
    /// A record of ours put, or deleted, in the group's space of a type.
    Put(&'static str, &'static str, At),
    /// Records of ours at their one key, in one space, each changed in
    /// the fields given of its own.
    Edit(&'static [&'static str], &'static str, &'static str),
    Delete(&'static str, &'static str, At),
    Invite,
    Grant,
    Label,
    Space,
}

/// The tools, by group: each its name, the arguments its command takes in
/// order after the group, what it does, and what it is made of.
const TOOLS: &[(&str, &[&str], &str, Made)] = &[
    (
        "group_show",
        &[],
        "The group as the member may see it.",
        Made::Method("internal.getGroup", None),
    ),
    (
        "group_deactivate",
        &[],
        "Switches the group off: nobody can read its spaces or write as it until one who \
         could switch it off switches it back on. With undo, switches it back on.",
        Made::Method("internal.deactivateGroup", Some("internal.activateGroup")),
    ),
    (
        "group_handle",
        &["handle"],
        "Gives the group a handle on a domain of its own, once the domain names the group: an \
         _atproto TXT record there saying did=<the group's DID>, or the DID at the domain's \
         /.well-known/atproto-did. Given the group's own address, gives it that back.",
        Made::Method("internal.updateGroupHandle", None),
    ),
    (
        "group_domain",
        &[],
        "Serves the group at its handle's own domain, such as riders.org, rather than its \
         address, once that domain reaches this host: www. under it and the group's address \
         redirect there. Refused while the handle is the address, or before the domain's A and \
         AAAA records name this host's addresses. With undo, serves it at its address alone \
         again.",
        Made::Method(
            "internal.putGroupDomain",
            Some("internal.deleteGroupDomain"),
        ),
    ),
    (
        "profile_set",
        &[],
        "Changes the group's profile as the group: its about record (org.atgrp.about), and its \
         config (org.atgrp.config) only when join_policy is given. Changes only the fields \
         given, keeping the rest; a field given empty, or null, is taken away.",
        Made::Edit(&["about", "config"], META, "self"),
    ),
    (
        "rule_set",
        &[],
        "Writes a rule as the group: the one `rule` names, replaced whole, or a new one.",
        Made::Put("rule", MEMBERS, At::New("rule")),
    ),
    (
        "rule_delete",
        &["rule"],
        "Deletes a rule as the group.",
        Made::Delete("rule", MEMBERS, At::Named("rule")),
    ),
    (
        "label_define",
        &["label"],
        "Defines one of the group's own labels, or changes one it defines: what it is called and \
         means, and how apps should show what wears it.",
        Made::Method("internal.putLabelDefinition", None),
    ),
    (
        "label_delete",
        &["label"],
        "Stops defining one of the group's own labels. Labels already applied stay.",
        Made::Method("internal.deleteLabelDefinition", None),
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
        "Deletes a role nobody holds, as the group.",
        Made::Delete("role", MEMBERS, At::Named("role")),
    ),
    (
        "role_grant",
        &["role"],
        "Adds to what a role may do across the group, as the group: the permissions given, and \
         the roles given that its holders may assign (* for every role), to those its record has; \
         with undo, takes them away, or with none given takes away all of both.",
        Made::Grant,
    ),
    (
        "member_list",
        &[],
        "Who is in the group, and their roles, a page at a time.",
        Made::Method("internal.listMembers", None),
    ),
    (
        "member_assign",
        &["member", "roles"],
        "Gives a member these roles in place of theirs, as the group (`member` among them), or \
         lets in someone waiting to join.",
        Made::Put("membership", MEMBERS, At::Did("member")),
    ),
    (
        "member_eject",
        &["member"],
        "Puts a member out, as the group.",
        Made::Delete("membership", MEMBERS, At::Did("member")),
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
         (org.atgrp.inbox); one already waiting for them is the one answered. With undo, \
         takes back the invite waiting for them.",
        Made::Invite,
    ),
    (
        "invite_list",
        &[],
        "The invites waiting.",
        Made::Method("internal.listInvites", None),
    ),
    (
        "space_create",
        &["type"],
        "Adds a space of a type to the group, under the policy given, as the group \
         (com.atproto.space.createSpace).",
        Made::Space,
    ),
    (
        "space_records",
        &["space"],
        "The records in one of the group's spaces.",
        Made::Method("internal.listSpaceRecords", None),
    ),
    (
        "mod_list",
        &[],
        "The moderation queue: every subject, or those of a status, or with any of the tags \
         given; oldest first, newest first, or the latest updated first.",
        Made::Method("mod.querySubjects", None),
    ),
    (
        "mod_history",
        &["subject"],
        "What was done about a subject, newest first or oldest first.",
        Made::Method("mod.querySubjectHistory", None),
    ),
    (
        "mod_update",
        &["subject"],
        "Resolves or reopens a subject, adds or removes its tags, or adds a note.",
        Made::Method("mod.updateSubject", None),
    ),
    (
        "mod_label",
        &["subject", "val"],
        "Labels a subject as the group, with a label record (com.atproto.label.record) beside \
         it: a record's in its space, an account's in the members space; where the member's \
         roles write labels, as getGroup's labelSpaces lists. With undo, takes the label back \
         by deleting it.",
        Made::Label,
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
                let action = Action::Method {
                    nsid,
                    procedure,
                    undo: opposite.map(|o| format!("{NS}.{o}")),
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
                    At::Did(n) => {
                        let said = format!("Whose {kind}: their DID.");
                        let did = json!({"type": "string", "format": "did", "description": said});
                        add(&mut input, n, did, true);
                        Key::Named(n.to_string())
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
            Made::Edit(kinds, space, rkey) => {
                let mut input = json!({"type": "object", "properties": {}, "required": []});
                let mut records = Vec::new();
                for kind in kinds.iter() {
                    let collection = format!("{NS}.{kind}");
                    let main = self.main(&collection);
                    let mut whole = self.object(&collection, &main["record"], DEPTH);
                    drop(&mut whole, "createdAt");
                    let fields = whole["properties"].as_object().cloned().unwrap_or_default();
                    let names: Vec<String> = fields.keys().cloned().collect();
                    for (name, property) in fields {
                        add(&mut input, &name, property, false);
                    }
                    records.push((collection, names));
                }
                let action = Action::Edit {
                    records,
                    space: space.to_string(),
                    rkey: rkey.to_string(),
                };
                (action, input, None)
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
            Made::Label => {
                let said =
                    "An account's DID, or a record's at:// URI in one of the group's spaces.";
                let subject = json!({"type": "string", "description": said});
                let said = "The record's version, its CID, if the label is about that one.";
                let cid = json!({"type": "string", "format": "cid", "description": said});
                let said = "The label's value: !hide, or one of the group's own.";
                let val = json!({"type": "string", "maxLength": 128, "description": said});
                let mut input = json!({"type": "object", "properties": {}, "required": []});
                add(&mut input, "subject", subject, true);
                add(&mut input, "val", val, true);
                add(&mut input, "cid", cid, false);
                (Action::Label, input, None)
            }
            Made::Space => {
                let policy = format!("{NS}.policy");
                let main = self.main(&policy);
                let mut input = json!({"type": "object", "properties": {}, "required": []});
                let said = "The space's type: an NSID, as com.example.board.";
                let kind = json!({"type": "string", "format": "nsid", "description": said});
                add(&mut input, "type", kind, true);
                add(
                    &mut input,
                    "policy",
                    self.object(&policy, main, DEPTH),
                    true,
                );
                let said = "Its key. Left out, a new one.";
                add(&mut input, "skey", rkey(said.into()), false);
                (Action::Space, input, main["description"].as_str())
            }
            Made::Grant => {
                let collection = format!("{NS}.role");
                let main = self.main(&collection);
                let whole = self.object(&collection, &main["record"], DEPTH);
                let mut input = json!({"type": "object", "properties": {}, "required": []});
                add(
                    &mut input,
                    "role",
                    rkey("Which role: its record's key.".into()),
                    true,
                );
                for field in ["permissions", "canAssign"] {
                    add(&mut input, field, whole["properties"][field].clone(), false);
                }
                (Action::Grant, input, main["description"].as_str())
            }
        };
        let undoes = matches!(
            action,
            Action::Invite | Action::Grant | Action::Label | Action::Method { undo: Some(_), .. }
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
            description: about.map_or(what.to_string(), |a| format!("{what} {a}")),
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
    /// Change each record at `rkey` in the group's space of the type
    /// `space`, by its collection, in the fields given, keeping the rest.
    /// `edited` makes each write, from the record as it is.
    Edit {
        space: String,
        rkey: String,
        records: Vec<(String, Map<String, Value>)>,
    },
    /// Label `subject` (an account's DID, or a record's URI, at the
    /// version `cid` if named) with `val`, as the group, beside it; with
    /// `undo`, delete the labels of that value on it there.
    Label {
        subject: String,
        cid: Option<String>,
        val: String,
        undo: bool,
    },
    /// Add `permissions` and `can_assign` to those of the role `role`; with
    /// `undo`, take them away, or with none take away all of both.
    /// `regrant` makes the write, from the role's record as it is.
    Grant {
        role: String,
        permissions: Vec<String>,
        can_assign: Vec<String>,
        undo: bool,
    },
    /// Make a space of the type `kind`, at `skey` (a new key if none),
    /// under `policy`, as the group.
    Space {
        kind: String,
        skey: Option<String>,
        policy: Value,
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
        } => {
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
            records,
            space,
            rkey,
        } => {
            let records: Vec<(String, Map<String, Value>)> = records
                .iter()
                .map(|(collection, names)| {
                    let fields = input.iter().filter(|(k, _)| names.contains(k));
                    let fields = fields.map(|(k, v)| (k.clone(), v.clone())).collect();
                    (collection.clone(), fields)
                })
                .filter(|(_, fields): &(String, Map<String, Value>)| !fields.is_empty())
                .collect();
            if records.is_empty() {
                return Err("nothing to change: give a field".into());
            }
            Plan::Edit {
                space: space.clone(),
                rkey: rkey.clone(),
                records,
            }
        }
        Action::Invite => {
            let person = given(&mut input, "person")?;
            let record = (!undo).then(|| stamped(input, &format!("{NS}.invite")));
            Plan::Invite { person, record }
        }
        Action::Label => Plan::Label {
            subject: given(&mut input, "subject")?,
            cid: given(&mut input, "cid").ok(),
            val: given(&mut input, "val")?,
            undo,
        },
        Action::Space => {
            let kind = given(&mut input, "type")?;
            let skey = given(&mut input, "skey").ok();
            let mut policy = input.remove("policy").unwrap_or_default();
            if !policy.is_object() {
                return Err("policy is required, as an object".into());
            }
            policy["$type"] = json!(format!("{NS}.policy"));
            Plan::Space { kind, skey, policy }
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
                permissions: list("permissions"),
                can_assign: list("canAssign"),
                undo,
            }
        }
    })
}

/// The label record's collection.
pub const LABEL: &str = "com.atproto.label.record";

/// The space of `group`'s a label on `subject` goes in: a record's own,
/// an account's or a public record's the members space. None for a
/// record in a space not the group's.
pub fn label_space(group: &str, subject: &str) -> Option<String> {
    let members = || format!("at://{group}/space/{MEMBERS}/self");
    if subject.starts_with("did:") {
        return Some(members());
    }
    let parts: Vec<&str> = subject.strip_prefix("at://")?.split('/').collect();
    match parts[..] {
        [_, _, _] => Some(members()),
        [authority, "space", kind, skey, _, _, _] => {
            (authority == group).then(|| format!("at://{authority}/space/{kind}/{skey}"))
        }
        _ => None,
    }
}

/// A label record on `subject`, at the version `cid` if named, of `val`,
/// made `now`.
pub fn label_record(subject: &str, cid: Option<&str>, val: &str, now: &str) -> Value {
    let mut record = json!({"$type": LABEL, "uri": subject, "val": val, "cts": now});
    if let Some(cid) = cid {
        record["cid"] = json!(cid);
    }
    record
}

/// Whether `record` is a label on `subject` of `val`.
pub fn labels(record: &Value, subject: &str, val: &str) -> bool {
    record["uri"] == subject && record["val"] == val
}

/// The write that adds `permissions` and `can_assign` to those of the role
/// `role`, in `kept`, its record as it is, or with `undo` takes them away,
/// or with none named takes away all of both: the rest of the record as
/// it was. Refused for a role the group does not have.
pub fn regrant(
    kept: &Value,
    role: &str,
    permissions: &[String],
    can_assign: &[String],
    undo: bool,
) -> Result<Plan, String> {
    let Value::Object(record) = kept else {
        return Err(format!("no such role: {role}"));
    };
    let mut record = record.clone();
    let named = !permissions.is_empty() || !can_assign.is_empty();
    for (field, given) in [("permissions", permissions), ("canAssign", can_assign)] {
        let had = record.get(field).and_then(Value::as_array).into_iter();
        let mut list: Vec<String> = Vec::new();
        for item in had.flatten().filter_map(Value::as_str).map(String::from) {
            if !list.contains(&item) {
                list.push(item);
            }
        }
        match undo {
            true if !named => list.clear(),
            true => list.retain(|i| !given.contains(i)),
            false => {
                let new: Vec<String> = given
                    .iter()
                    .filter(|g| !list.contains(g))
                    .cloned()
                    .collect();
                list.extend(new);
            }
        }
        match list.is_empty() {
            true => record.remove(field),
            false => record.insert(field.into(), json!(list)),
        };
    }
    record.insert("$type".into(), json!(format!("{NS}.role")));
    Ok(Plan::Write {
        space: MEMBERS.into(),
        collection: format!("{NS}.role"),
        rkey: Some(role.into()),
        record: Some(Value::Object(record)),
    })
}

/// The write that changes `kept`, the record of the kind `collection` at
/// `rkey` in the group's space of the type `space` as it is (none if there
/// is none), in the `fields` given: each set, or taken away if given
/// empty or null; the rest as they were.
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
