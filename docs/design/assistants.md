# Assistants

A member may want an AI assistant to do group work for them: admit
people, write rules, look through the moderation queue. The assistant
should act with exactly that member's power in one group, and no more.
It should stop when the member disconnects it or leaves the group.

An assistant here is an AI app that speaks MCP, the protocol an
assistant uses to call tools on a server. The host is its MCP server and
its OAuth server. The member connects it to one group, and from then on
it acts in that group as that member.

## What an assistant may do

An assistant asks for no permissions of its own. If it held its own
grant, that grant could drift from what the member may do. Instead, what
it may do is decided at each call by the member's roles in the group as
they are at that moment (see [membership](membership.md)). A role given
to the member widens what the assistant can do; a role taken away
narrows it, on the next call.

An assistant is connected to exactly one group, chosen by the member
when they approve it. No tool asks which group. The host tells the
assistant, when it connects, that its tools act as the person who
connected it, in the group they chose, and that anything their roles do
not allow is refused.

To approve an assistant, the member must hold a role in the group, and
the group must be switched on. The approval page offers only such
groups.

## Connecting and signing in

The host answers MCP at `<PUBLIC_URL>/mcp` and runs the OAuth flow MCP's
authorization spec describes. An assistant that calls `/mcp` without a
token gets a `401` pointing to the host's metadata, and from there finds
where to sign in.

An assistant names itself in one of two ways. It can give the https URL
of a client metadata document, a JSON file that describes it; the host
fetches that document at each sign-in and stores nothing. Or it can
register at `/mcp/register` as a public client, with no secret;
registration is open, and a registered client never connected is
forgotten after a day. One a member has connected is kept, to connect
again, and its connections are forgotten once they lapse.

The member approves on the console. The assistant sends the member's
browser to `/mcp/authorize`, and the host passes them to the console's
approval page. Signed in to the console as themselves, the member sees
the assistant's name, the client ID it came from, and the roles it would
act with, then picks a group or declines. The name is the assistant's
own word for itself and the host does not check it, so the page says so
and shows the client ID beside it.

Approval sends the browser back to the assistant with a code. The
assistant exchanges it once, proving with PKCE that it started the flow,
for an access token and a refresh token.

Apps sign in to a group through a different path, the host's atproto
OAuth server, where they act as the group itself (see [signing
in](signing-in.md)). An assistant acts as a member, so the two must not
mix. Assistants' tokens are kept apart from apps' tokens. `/mcp` accepts
only an assistant's token, and the XRPC methods never accept one.

Details:

- The metadata is at `/.well-known/oauth-protected-resource/mcp` and
  `/.well-known/oauth-authorization-server/mcp`; the issuer is
  `<PUBLIC_URL>/mcp`.
- PKCE is `S256` only. Grants are `authorization_code` and
  `refresh_token`.
- A pending request lasts 10 minutes. An access token lasts an hour.
- Refreshing rotates both tokens, and the old refresh token works once.
  A refresh token lapses 30 days after it was issued, so an assistant
  that has not refreshed for 30 days must be approved again.
- Tokens are random and kept only as SHA-256 hashes.
- Redirect URIs, limits and errors are in [the API
  reference](../reference/api.md).

## Tools and calls

The tools are the host's own methods for running a group, 21 of them,
each named after its method: the group's profile and rules, roles, join
requests and invites, spaces, and the moderation queue and labels (see
[moderation](moderation.md)). Each tool's description is its method's
lexicon description, and its input schema is made from the lexicon,
without the `group` parameter. [The MCP reference](../reference/mcp.md)
lists them.

The safest way to keep an assistant within the member's power is to give
it no other path than the member's own. So a tool call is the host
asking itself, as the member, the way the console does. It fills in the
connected group, makes a console session for the member, and sends the
method's request through its own router. The method then decides by the
member's roles, as it would for any caller. A refusal comes back as the
tool's error, with the method's error and message; a success comes back
as the method's JSON.

Some methods are not tools. An assistant cannot list or end connections,
so it cannot hide itself or end another assistant's. It cannot change
the group's recovery keys or delete the group (see
[recovery](recovery.md)); those stay with the member, on the console.

Details:

- `/mcp` takes one JSON-RPC 2.0 message per POST and answers it as JSON.
  A notification is answered `202` with no body.
- It answers `initialize`, `ping`, `tools/list` and `tools/call`.
- Signing out of the console does not stop a member's assistants, since
  each call makes its own session.

## Seeing and disconnecting

On the group's page, a member sees "Your assistants": the ones they
connected to that group, with when each connected and was last used.
With none connected, the page shows the MCP address to add. A member
sees and ends only their own.

Disconnecting an assistant with `revokeConnectedApp` ends every
connection the member made with that client in that group, and its
tokens with it. A connection also ends when:

- the member is put out of the group, by ejection or by leaving;
- the group is deleted, which removes its connections and pending
  requests too;
- its refresh token lapses.
