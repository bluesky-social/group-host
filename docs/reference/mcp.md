# MCP tools

The tools the host offers an assistant at `<PUBLIC_URL>/mcp`, over MCP's
Streamable HTTP, each JSON-RPC message answered on its own, as JSON. It
speaks MCP `2025-06-18`, `2025-03-26` and `2024-11-05`, and answers an
`initialize` asking for another with the latest.
[Assistants](../design/assistants.md) says how an assistant connects and
signs in.

| Message | Answer |
| --- | --- |
| `initialize`, `ping`, `tools/list`, `tools/call` | as MCP has them |
| a notification, with no `id` | 202, with no body |
| any other method | JSON-RPC `-32601` |
| a body that is not JSON | 400, JSON-RPC `-32700` with a null `id` |
| JSON that is not one JSON-RPC 2.0 object (a batch, a string) | `-32600` |
| an unknown tool, or arguments that are not an object | `-32602` |
| no token, or one the host does not know | 401, with `WWW-Authenticate: Bearer` naming the resource metadata |

Each tool is one of the host's methods, named as the method is
(`getGroup` for `fyi.opensocial.getGroup`), asked as the member who
connected the assistant, in the group they connected it to. Its input
schema is made from the method's lexicon (`METHODS` in
`crates/group-host-mcp/src/lib.rs` lists them), without the group, which
is always that one; what a tool takes and answers is the method's, in
[the API](api.md). A tool's description is its method's lexicon's. Its
result is the method's JSON, as text and, for an object, as
`structuredContent`. A call the member's roles do not allow, or the method
refuses, is `isError: true`, its text `<error>: <message>` in the method's
words.

Ten tools write a record instead of calling a method, each the write
the member's own session for the group would make, with the scope a
group token for that space carries and the member's roles as they are,
logged under the assistant.

`sendInvite` and `takeBackInvite` make the write in the invitee's inbox
(`com.atproto.space.createRecord` and `deleteRecord`). Each answers the
invite's `uri`.

`putProfile` (`fyi.opensocial.profile`, at `self`), `putRule` and
`deleteRule` (`fyi.opensocial.rule`) write the group's own records in
`meta`, held to the same rules as any write of them
([groups](../design/groups.md#changing-its-profile-rules-and-pictures)).
`putRole` and `deleteRole` (`fyi.opensocial.role`, at the role's id),
`putPermissions` (`fyi.opensocial.permissions`, at `self`), and
`putMembership` and `deleteMembership` (`fyi.opensocial.membership`, at
the member's DID) write those in `members`, held to the same rules
([membership](../design/membership.md#writing-roles-grants-and-memberships)).
A put's input schema is the record's lexicon, and replaces the record
whole; a tool for records not kept at one key takes an `rkey`: to change
one, and for a rule without one makes a new rule, keyed by the host's
clock; a delete takes the `rkey`. Their descriptions are the record's
lexicon's, after what the tool does, and each answers the record's
`uri`, and for a put its `cid`. `RECORDS` in
`crates/group-host-mcp/src/lib.rs` lists them. A picture cannot be
uploaded through MCP; a profile put keeps a picture only by naming a
blob the group already holds.

| Tool | What it does |
| --- | --- |
| `getGroup` | the group as the member may see it: its profile, rules, how many are in it, roles and spaces |
| `listMembers` | who is in the group, and their roles, a page at a time, found by part of a handle or DID (`q`) |
| `putProfile` | replaces the group's profile, as the group |
| `putRule`, `deleteRule` | makes, changes or removes a rule, as the group |
| `putRole`, `deleteRole` | writes a role, or deletes one nobody holds or is granted, as the group |
| `putPermissions` | writes the permissions record, each role's scopes and the roles it may assign, as the group |
| `putMembership` | writes a membership as the group: lets in someone waiting to join, or changes a member's roles |
| `listJoinRequests`, `denyJoinRequest` | the requests to join waiting, and turning one down |
| `listInvites` | the invites waiting |
| `sendInvite`, `takeBackInvite` | invites someone (`invitee`, with an optional `message` and `expiresAt`), or takes back the invite waiting for them |
| `deleteMembership` | puts a member out |
| `createSpace` | adds a space to the group |
| `listSpaceRecords` | the records in one of the group's spaces |
| `listSubjects`, `getSubjectHistory`, `resolveSubject` | the moderation queue, a subject's events, and resolving or escalating one |
| `applyLabel`, `negateLabel` | labels a subject, or takes a label back |
