# Context to give a writer

Copy what the page needs into its context file. Give only the glossary
terms its facts use: a writer given more atproto than that teaches the
protocol instead of the page.

## The group host in three lines

- A group is an atproto account of its own, with no password and no one
  person behind it. This host is its PDS, the authority for its private
  spaces, and its OAuth server.
- Members keep their own accounts on their own PDSes, anywhere. What a
  member may do in a group is what their roles there grant, and an app or
  assistant a member signs in gets what those roles allow and nothing more.
- The host is one Rust server over SQLite, with a console (a SvelteKit
  app) where people start and run groups and approve apps, and a command,
  `opensocial`, that does the same from a terminal.

## The reader

Someone who knows atproto roughly: accounts keep records in repos on a
server called a PDS, and are named by DIDs and handles. They are running
a group host, building an app that talks to one, or changing this one.
They do not know the permissioned-data spaces in detail. Explain a term
the first time it matters, in a clause, and link to https://atproto.com
for more.

## Glossary

- **PDS**: the server that holds an account's repo and blobs, and signs
  its commits.
- **DID**: an account's permanent ID (`did:plc:…`, or `did:web:…`). A
  **handle** is a changeable name that resolves to it. A **PLC
  operation** changes a `did:plc` document, signed by a **rotation key**.
- **record / collection / rkey**: a JSON document in a collection (an
  NSID such as `fyi.opensocial.rule`) in an account's repo, addressed by a
  key.
- **blob**: binary data stored beside records (an avatar, a file).
- **space**: a private record store from atproto's permissioned-data
  alpha. Each member writes their records in their own repo in the space,
  on their own PDS; the space's **authority** decides who may read and
  write, and gives readers **credentials**.
- **service auth**: a short-lived token an account's PDS signs for
  calling another service as that account (`aud` the service, `lxm` the
  method).
- **OAuth / DPoP**: atproto's OAuth, where a client signs in to an
  account at its authorization server; DPoP binds each token to a key the
  client holds.
- **lexicon**: the schema for a record or method (`lexicons/fyi/opensocial/`).
- **label**: a moderation tag on a record or account.
- **MCP**: the protocol an AI assistant uses to call tools on a server.

## Pages to link

Design: `README.md` (overview), `spaces.md`, `groups.md`,
`membership.md`, `signing-in.md`, `moderation.md`, `assistants.md`,
`recovery.md`, `moving.md`. Reference: `../reference/api.md`, `mcp.md`,
`cli.md`, `config.md`. `../development.md`. Link to them rather than
restating them.
