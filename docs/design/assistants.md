# Assistants

An assistant, an AI app that is an MCP client, connects to a group to act
there as the member who connected it, with what their roles allow and
nothing more (`assistants.rs`, and `crates/group-host-mcp` for the
protocol and the tools).

## Connecting

The host is the assistant's MCP server, at `<PUBLIC_URL>/mcp`, and the
OAuth server it signs in at, as MCP's authorization spec has it:

- `/.well-known/oauth-protected-resource/mcp` names the resource and its
  authorization server, whose issuer is `<PUBLIC_URL>/mcp`, and whose
  metadata is at `/.well-known/oauth-authorization-server/mcp`.
- An assistant names itself by a client ID metadata document, its client
  ID that document's https URL, or registers at `/mcp/register` (RFC 7591)
  as a public client. Either way it names at most ten redirect URIs, each
  one it may be sent back to: over https, to a loopback address over
  http, or to an app's own scheme, never a script's. A client registered
  and never connected is forgotten after a day. Its name is its own word
  for itself, and the approval page says so, with where it came from.
- `/mcp/authorize` takes a code request with PKCE (S256) for that
  resource, and sends the browser to the console's approval page, where
  the member, signed in as themselves, reads it with `getAuthorization`
  (`assistant: true`), and picks the group to connect it to.
- `/mcp/token` exchanges the code, once, for its client, redirect URI and
  verifier, for a Bearer access token (an hour) and a refresh token (thirty
  days unused), each kept by its hash; a refresh token is used once.

This is not the host's atproto OAuth server, where apps sign in *as* a
group (see [signing in](signing-in.md)). An assistant's tokens are kept
apart from a group's, and neither is taken for the other.

## Acting

Each message an assistant sends to `/mcp` (JSON-RPC, each answered on its
own, as JSON) is answered as the member, in the group they chose. Its
tools are the core's methods, each by its lexicon (`getGroup`,
`createInvite`, `putRule`, `applyLabel`, …), their input schemas made from
the lexicons, without the group, which is always the one chosen. A call is
the host asking itself as the member, as the console does: the host's
methods decide, by the member's roles as they are now, and a refusal is
the tool's error, in the method's words.

Each member sees, on the group's page, the assistants they connected, and
disconnects one there (`listConnectedApps`, `revokeConnectedApp`). Put out
of the group, the member's assistants are disconnected with them.

## From the prototype

- The prototype's assistant was its sites module's, at each group's own
  address, and made pages; this one is the core's methods alone, at the
  host's address, and the member chooses the group as they connect it.
