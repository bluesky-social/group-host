# MCP

A member may want an AI app to do group work for them: admit people,
write rules, look through the moderation queue. The app should act with
exactly that member's power in one group, and no more. It should stop
when the member disconnects it or leaves the group.

The host is an MCP server for that, MCP being the protocol an AI app
uses to call tools on a server, and the OAuth server its client signs in
at. The member connects the client to one group, and from then on it
acts in that group as that member. The console calls such a client an
AI app; this page calls it an MCP client.

## What an MCP client may do

An MCP client asks for no permissions of its own. If it held its own
grant, that grant could drift from what the member may do. Instead, what
it may do is decided at each call by the member's roles in the group as
they are at that moment (see [membership](membership.md)). A role given
to the member widens what the client can do; a role taken away narrows
it, on the next call.

A client is connected to exactly one group, chosen by the member when
they approve it. No tool asks which group. The host tells the client,
when it connects, that its tools act as the person who connected it, in
the group they chose, and that anything their roles do not allow is
refused.

To approve a client, the member must hold a role in the group, and the
group must be switched on. The approval page offers only such groups.

## Connecting and signing in

The host answers MCP at `<PUBLIC_URL>/mcp` and runs the OAuth flow MCP's
authorization spec describes. A client that calls `/mcp` without a token
gets a `401` pointing to the host's metadata, and from there finds where
to sign in.

A client names itself in one of two ways. It can give the https URL of a
client metadata document, a JSON file that describes it; the host
fetches that document at each sign-in and stores nothing. Or it can
register at `/mcp/register` as a public client, with no secret;
registration is open, and a registered client never connected is
forgotten after a day. One a member has connected is kept, to connect
again, and its connections are forgotten once they lapse.

The member approves on the console. The client sends the member's
browser to `/mcp/authorize`, and the host passes them to the console's
approval page. Signed in to the console as themselves, the member sees
the client's name, the client ID it came from, and the roles it would
act with, then picks a group or declines. The name is the client's own
word for itself and the host does not check it, so the page says so and
shows the client ID beside it.

Approval sends the browser back to the client with a code. The client
exchanges it once, proving with PKCE that it started the flow, for an
access token and a refresh token.

Apps sign in to a group through a different path, the host's atproto
OAuth server, where they act as the group itself (see [signing
in](signing-in.md)). An MCP client acts as a member, so the two must not
mix. MCP clients' tokens are kept apart from apps' tokens. `/mcp`
accepts only an MCP client's token, and the XRPC methods never accept
one.

Details:

- The metadata is at `/.well-known/oauth-protected-resource/mcp` and
  `/.well-known/oauth-authorization-server/mcp`; the issuer is
  `<PUBLIC_URL>/mcp`.
- PKCE is `S256` only. Grants are `authorization_code` and
  `refresh_token`.
- A pending request lasts 10 minutes. An access token lasts an hour.
- Refreshing rotates both tokens, and the old refresh token works once.
  A refresh token lapses 30 days after it was issued, so a client that
  has not refreshed for 30 days must be approved again.
- Tokens are random and kept only as SHA-256 hashes.
- Redirect URIs, limits and errors are in [the API
  reference](../reference/api.md).

## Tools and calls

Each tool is the twin of a command of `opensocial`
([the commands](../reference/cli.md#opensocial)): `opensocial member
assign` is the tool `member_assign`, and takes the same arguments, its
flags named in snake case. The two are made from one list of tools in
`crates/group-host-mcp`, so they cannot drift. The tools are grouped as
the commands are: the group, its profile, rules, roles, members, join
requests, invites, spaces, and moderation (see
[moderation](moderation.md)). Each tool's input schema is made from the
lexicon of what it asks or writes, without the group, and its
description says what it does, then what that lexicon says. [The MCP
reference](../reference/mcp.md) lists them.

What a call does is worked out the same way for a tool and its command,
as a plan of one of four kinds:

- **A method of the host's**, such as `listInvites` for `invite_list`.
  The host asks itself, as the member, the way the console does: it
  fills in the connected group, makes a console session for the member,
  and sends the method's request through its own router. The method then
  decides by the member's roles, as it would for any caller.
- **A record of the group's**, such as a rule, a role or a membership,
  put or deleted in its meta or members space. The host makes the write
  as the member's own session for the group would make it: with the
  scope a group token for that space carries, the member's roles as they
  are, and the same rules as any write of it
  ([membership](membership.md#writing-roles-grants-and-memberships)).
- **An invite**, written as the group in the invitee's inbox, or taken
  back, in the same way, with the scope a group token for that inbox
  carries ([membership](membership.md#invites)).
- **A role's grant**, set or dropped in the permissions record, the rest
  of the record as it was.

The safest way to keep an MCP client within the member's power is to
give it no other path than the member's own, and each kind is that. A
refusal comes back as the tool's error, with the host's error and
message; a success comes back as the method's JSON, or the record's URI.

Some of what the host does is not a tool. An MCP client cannot list or
end connections, so it cannot hide itself or end another client's. It
cannot make a group, move one, or bring an account here, which are
commands only, nor change the group's recovery keys or delete it (see
[recovery](recovery.md)); those stay with the member.

Details:

- `/mcp` takes one JSON-RPC 2.0 message per POST and answers it as JSON.
  A notification is answered `202` with no body.
- It answers `initialize`, `ping`, `tools/list` and `tools/call`.
- Signing out of the console does not stop a member's MCP clients, since
  each call makes its own session.

## Seeing and disconnecting

On the group's page, a member whose roles hold `group.configure` sees
"Connect an AI app": the MCP address to add, to copy, what the app will
then ask of them, and a few things to ask it; and under it the clients
they connected to that group, with when each connected and was last
used. Any other member sees theirs under "Your AI apps", once they have
connected one. A member sees and ends only their own.

Disconnecting a client with `revokeConnectedApp` ends every connection
the member made with that client in that group, and its tokens with it.
A connection also ends when:

- the member is removed from the group, by ejection, or leaves it;
- the group is deleted, which removes its connections and pending
  requests too;
- its refresh token lapses.
