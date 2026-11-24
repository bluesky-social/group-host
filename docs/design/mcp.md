# MCP

A member may want an AI app to do group work for them, like admitting
people, writing rules or looking through the moderation queue. The app
should act with exactly that member's power in one group, and no more.
It should stop when the member disconnects it or leaves the group.

MCP is the protocol an AI app uses to call tools on a server. The host
is an MCP server for this work, and it's also the OAuth server the
client signs in at. The member connects the client to one group, and
from then on it acts in that group as that member. The console calls
such a client an AI app, and this page calls it an MCP client.

## What an MCP client may do

An MCP client asks for no permissions of its own. If it held its own
grant, that grant could drift from what the member may do. Instead, the
host decides what it may do at each call, from the member's roles in the
group at that moment (see [membership](membership.md)). If the member
gets a role, the client can do more on its next call. If a role is taken
away, the client can do less.

A client is connected to exactly one group, which the member chooses
when they approve it. No tool asks which group. When the client
connects, the host tells it that its tools act as the person who
connected it, in the group they chose. It also tells it that the host
refuses anything their roles don't allow.

To approve a client, the member must hold a role in the group, and the
group must be switched on. The approval page offers only such groups.

## Connecting and signing in

The host answers MCP at `<PUBLIC_URL>/mcp` and runs the OAuth flow MCP's
authorization spec describes. A client that calls `/mcp` without a token
gets a `401` pointing to the host's metadata, and from there it finds
where to sign in.

A client names itself in one of two ways. It can give the https URL of a
client metadata document, a JSON file that describes it. The host
fetches that document at each sign-in and stores nothing. Or it can
register at `/mcp/register` as a public client, with no secret.
Registration is open, and the host forgets a registered client that's
never connected after a day. It keeps one a member has connected, so it
can connect again, and forgets its connections once they lapse.

The member approves on the console. The client sends the member's
browser to `/mcp/authorize`, and the host passes them to the console's
approval page. The member is signed in to the console as themselves.
They see the client's name, the client ID it came from, and the roles it
would act with, and then they pick a group or decline. The name is the
client's own word for itself and the host doesn't check it. So the page
says so and shows the client ID beside it.

Approval sends the browser back to the client with a code. The client
exchanges it once for an access token and a refresh token, proving with
PKCE that it started the flow.

Apps sign in to a group through a different path, the host's atproto
OAuth server, where they act as the group itself (see [signing
in](signing-in.md)). An MCP client acts as a member, so the two must not
mix. The host keeps MCP clients' tokens apart from apps' tokens. `/mcp`
accepts only an MCP client's token, and the XRPC methods never accept
one.

Details:

- The metadata is at `/.well-known/oauth-protected-resource/mcp` and
  `/.well-known/oauth-authorization-server/mcp`. The issuer is
  `<PUBLIC_URL>/mcp`.
- PKCE is `S256` only. Grants are `authorization_code` and
  `refresh_token`.
- A pending request lasts 10 minutes. An access token lasts an hour.
- Refreshing rotates both tokens, and the old refresh token works once.
  A refresh token lapses 30 days after it was issued, so a client that
  hasn't refreshed for 30 days must be approved again.
- Tokens are random, and the host keeps them only as SHA-256 hashes.
- Redirect URIs, limits and errors are in [the API
  reference](../reference/api.md).

## Tools and calls

Each tool is the twin of a command of `atgrp`
([the commands](../reference/cli.md#atgrp)). For example,
`atgrp member assign` is the tool `member_assign`. It takes the
same arguments, with its flags named in snake case. Both come from one
list of tools in `crates/group-host-mcp`, so they can't drift. The tools
are grouped the way the commands are: the group, its profile, rules,
roles, members, join requests, invites, spaces, and moderation (see
[moderation](moderation.md)). Each tool's input schema comes from the
lexicon of what it asks or writes, without the group. Its description
says what it does, then what that lexicon says. [The MCP
reference](../reference/mcp.md) lists them.

The host works out what a call does the same way for a tool and its
command. It makes a plan of one of four kinds:

- A method of the host's, such as `listInvites` for `invite_list`. The
  host asks itself, as the member, the way the console does. It fills in
  the connected group, makes a console session for the member, and sends
  the method's request through its own router. The method then decides
  by the member's roles, as it would for any caller.
- A record of the group's, such as a rule, a role or a membership, put
  or deleted in its meta or members space. The host makes the write the
  way the member's own session for the group would. It uses the scope a
  group token for that space carries, the member's current roles, and
  the same rules as any write of that record
  ([membership](membership.md#writing-roles-and-memberships)).
- An invite, written as the group in the invitee's inbox, or taken back
  the same way. It uses the scope a group token for that inbox carries
  ([membership](membership.md#invites)).
- A role's grant, added to or taken from in the role's own record. The
  rest of the record stays as it was.

The safest way to keep an MCP client within the member's power is to
give it no path but the member's own, and each kind is that path. A
refusal comes back as the tool's error, with the host's error and
message. A success comes back as the method's JSON, or the record's URI.

Some of what the host does isn't a tool. An MCP client can't list or end
connections, so it can't hide itself or end another client's. It can't
make a group, move one, or bring an account here, since those are
commands only. It also can't change the group's recovery keys or delete
it (see [recovery](recovery.md)). Those stay with the member. It can
switch the group off, as `group_deactivate`, and a steward can undo
that.

Details:

- `/mcp` takes one JSON-RPC 2.0 message per POST and answers it as JSON.
  A notification gets `202` with no body.
- It answers `initialize`, `ping`, `tools/list` and `tools/call`.
- Signing out of the console doesn't stop a member's MCP clients, since
  each call makes its own session.

## Seeing and disconnecting

On the group's page, a member whose roles hold `group.configure` sees
"Connect an AI app". It shows the MCP address to add and copy, what the
app will then ask of them, and a few things to ask it.
Under it are the clients they connected to that group, with when each
connected and when it was last used. Any other member sees theirs under
"Your AI apps", once they've connected one. A member sees and ends only
their own.

Disconnecting a client with `revokeConnectedApp` ends every connection
the member made with that client in that group, along with its tokens.
A connection also ends when:

- the member is removed from the group by ejection, or leaves it
- the group is deleted, which also removes its connections and pending
  requests
- its refresh token lapses
