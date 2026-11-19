# MCP tools

The tools the host offers an MCP client at `<PUBLIC_URL>/mcp`, over
MCP's Streamable HTTP, each JSON-RPC message answered on its own, as
JSON. It speaks MCP `2025-06-18`, `2025-03-26` and `2024-11-05`, and
answers an `initialize` asking for another with the latest.
[MCP](../design/mcp.md) says how a client connects and signs in.

| Message | Answer |
| --- | --- |
| `initialize`, `ping`, `tools/list`, `tools/call` | as MCP has them |
| a notification, with no `id` | 202, with no body |
| any other method | JSON-RPC `-32601` |
| a body that is not JSON | 400, JSON-RPC `-32700` with a null `id` |
| JSON that is not one JSON-RPC 2.0 object (a batch, a string) | `-32600` |
| an unknown tool, or arguments that are not an object | `-32602` |
| no token, or one the host does not know | 401, with `WWW-Authenticate: Bearer` naming the resource metadata |

## The tools are the commands

Every command `opensocial <group> <action>` that acts in a group has one
tool, `<group>_<action>`: `opensocial member assign` is `member_assign`.
A tool takes what its command takes, but the group, which is always the
one the client was connected to: the arguments the command takes in
order, named as its usage names them (`member`, `roles`), and its flags,
in snake case (`--display-name` is `display_name`, `--undo` is `undo`).
A list the command takes comma-separated, a tool takes as an array.
[The commands](cli.md#opensocial) have each one's arguments; `TOOLS` in
`crates/group-host-mcp/src/lib.rs` lists them, and the command reads its
words by the tool's schema, so the two cannot drift.

The commands with no tool make, move or delete a group or an account:
`group create`, `group seed`, `group move`, `group delete` and `account
import`. A client connected to a group can switch it off, which its
stewards undo, but never delete it. Nor have `admin logs` and `admin
stats`, which read the whole host, while a client acts in one group.

Each tool is asked as the member who connected the client, in that
group, and does one of four things: asks one of the host's methods, as
the member; writes a record of the group's, a profile, rule, role or
membership, as the member's own session for the group would, with the
scope a group token for that space carries and the member's roles as
they are, logged under the client; writes an invite in the invitee's
inbox, or takes it back, the same way; or sets a role's grant in the
permissions record. [MCP](../design/mcp.md#tools-and-calls) says why.

Its input schema is made from the lexicon of what it asks or writes,
named in the Lexicon column below, without the group and without when a
record was made, which the host says. Its description says what it does,
then what that lexicon says. A method's result is the method's JSON, a
write's the record's `uri`, and for a put its `cid`; each as text and,
for an object, as `structuredContent`. A call the member's roles do not
allow, or the host refuses, is `isError: true`, its text `<error>:
<message>` in the host's words. An argument a tool requires and was not
given, or not as text, is refused before anything is asked:
`<name> is required, as text`.

A write replaces the record whole, a field not given dropped, but
`profile_set` and `role_grant`, which change only what they are given.
A picture cannot be uploaded through MCP; `profile_set` keeps the
group's as they are. A link `profile_set` writes that the profile did
not have is given its site's icon, which the host asks of the favicon
service `FAVICONS` names and keeps as the group's; a link it had keeps
its icon.

## group

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `group_show` | `group show` | `fyi.opensocial.getGroup` | the group as the member may see it: its profile, rules, how many are in it, roles and spaces |
| `group_deactivate` | `group deactivate` | `fyi.opensocial.deactivateGroup` | switches the group off; with `undo`, `fyi.opensocial.activateGroup` switches it back on |
| `group_handle` | `group handle` | `fyi.opensocial.updateGroupHandle` | gives the group a handle on a domain that names it, or its own address back |

## profile

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `profile_set` | `profile set` | `fyi.opensocial.profile` | changes the group's profile, at `self` in its meta space, as the group, in the fields given, keeping the rest; a field given empty, an empty list or `null` is taken away |

## rule

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `rule_set` | `rule set` | `fyi.opensocial.rule` | writes a rule in the meta space, as the group: the one `rule` names, by its key, or without one a new one, keyed by the host's clock |
| `rule_delete` | `rule delete` | `fyi.opensocial.rule` | deletes the rule `rule` names |

## label

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `label_define` | `label define` | `fyi.opensocial.putLabelDefinition` | defines the group's own label `label`, or changes it: its `name` and `description`, and how apps should show it (`severity`, `blurs`, `default_setting`, `adult_only`) |
| `label_delete` | `label delete` | `fyi.opensocial.deleteLabelDefinition` | stops defining the label `label` |

## role

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `role_set` | `role set` | `fyi.opensocial.role` | writes the role `role`, its id, in the members space, as the group |
| `role_delete` | `role delete` | `fyi.opensocial.role` | deletes a role nobody holds or is granted |
| `role_grant` | `role grant` | `fyi.opensocial.permissions` | adds the `scopes` given, and the roles given that its holders may assign (`assignable`), to the grant of `role` in the permissions record; with `undo`, takes them away, or with none given drops the grant. The rest of the record is as it was, and the write is held to the same rules as any of it |

## site

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `site_publish` | `site publish` | `fyi.opensocial.putSite` | publishes the group's website whole, from `files`, each `{path, text}` or `{path, bytes}`, with `fallback` and `not_found` naming files of them, `reads` the host's methods its pages may ask as whoever signs in, `acts` the procedures they may ask as that person, and `writes` the records they may write in that person's own repo, each `{space, collection}`; the command reads the files from a folder |
| `site_delete` | `site delete` | `fyi.opensocial.deleteSite` | takes the group's website down |
| `site_domain` | `site domain` | `fyi.opensocial.putSiteDomain` | serves the group's website at its handle's own domain once it reaches this host; with `undo`, `fyi.opensocial.deleteSiteDomain`, at its address alone again |

## member

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `member_list` | `member list` | `fyi.opensocial.listMembers` | who is in the group, and their roles, a page at a time, found by part of a handle or DID (`q`) |
| `member_assign` | `member assign` | `fyi.opensocial.membership` | writes the membership of `member`, a DID, with `roles` in place of theirs (`member` among them): lets in someone waiting to join, or changes a member's roles |
| `member_eject` | `member eject` | `fyi.opensocial.membership` | deletes it, putting them out |

## join

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `join_list` | `join list` | `fyi.opensocial.listJoinRequests` | the requests to join waiting |
| `join_deny` | `join deny` | `fyi.opensocial.denyJoinRequest` | turns `member`'s request down |

## invite

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `invite_send` | `invite send` | `fyi.opensocial.invite` | invites `person`, a DID, with an optional `message` and `expires_at`, answering the invite's `uri`; with `undo`, takes back the invite waiting for them |
| `invite_list` | `invite list` | `fyi.opensocial.listInvites` | the invites waiting |

## space

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `space_create` | `space create` | `fyi.opensocial.createSpace` | adds a space of `type` to the group |
| `space_records` | `space records` | `fyi.opensocial.listSpaceRecords` | the records in the space `space` |

## mod

`subject` is an account's DID or a record's `at://` URI, with its `cid`
where the method asks for a version, as a label on a record does.

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `mod_list` | `mod list` | `fyi.opensocial.listSubjects` | the moderation queue |
| `mod_history` | `mod history` | `fyi.opensocial.getSubjectHistory` | what was done about a subject |
| `mod_resolve` | `mod resolve` | `fyi.opensocial.resolveSubject` | resolves or escalates a subject |
| `mod_label` | `mod label` | `com.atproto.space.createRecord` | labels a subject `val` as the group, with a `com.atproto.label.label` record beside it; with `undo`, deletes it (`com.atproto.space.deleteRecord`) |
