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

Two tools write a record instead: `sendInvite` and `takeBackInvite`
make the write in the invitee's inbox that the member's own session for
the group would (`com.atproto.space.createRecord` and `deleteRecord`),
with the scope a group token for that inbox carries and the member's
roles as they are, logged under the assistant. Each answers the invite's
`uri`.

| Tool | What it does |
| --- | --- |
| `getGroup` | the group as the member may see it: its profile, rules, roster, roles and spaces |
| `updateProfile` | replaces the group's profile |
| `putRule`, `deleteRule` | makes, changes or removes a rule |
| `putRole`, `deleteRole` | makes, changes or removes a role |
| `assignRoles` | sets a member's roles |
| `listJoinRequests`, `admitMember`, `denyJoinRequest` | the requests to join waiting, and letting one in or turning it down |
| `listInvites` | the invites waiting |
| `sendInvite`, `takeBackInvite` | invites someone (`invitee`, with an optional `message` and `expiresAt`), or takes back the invite waiting for them |
| `ejectMember` | puts a member out |
| `createSpace` | adds a space to the group |
| `listSpaceRecords` | the records in one of the group's spaces |
| `listSubjects`, `getSubjectHistory`, `resolveSubject` | the moderation queue, a subject's events, and resolving or escalating one |
| `applyLabel`, `negateLabel` | labels a subject, or takes a label back |
