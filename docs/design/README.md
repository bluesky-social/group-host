# Design

A person on atproto has a PDS: the server that keeps their repo, signs
their commits, and lets apps sign in as them. A group needs the same, and
more. It has no one person behind it, so what it may do has to be decided
by its members' roles. Much of what it keeps is private, so someone has to
decide who reads it. This host does both for every group it holds.

It implements the opensocial proposal
(https://tangled.org/opensocial.group/proposal). Members keep their own
accounts on their own PDSes, anywhere on the network.

## What the host is to a group

- Its PDS. Each group is an account here, with its own `did:plc`, public
  repo and files ([groups](groups.md)).
- The authority for its spaces, the private record stores where the group
  and its members write. The host decides who reads and writes, gives
  readers credentials, and keeps an index of what was written
  ([spaces](spaces.md)).
- Its OAuth server. A member signs an app in as the group, and the app
  gets what that member's roles allow, checked again at every write
  ([signing in](signing-in.md)).
- An MCP server, where a member connects an AI app to act for them in
  one group, its tools the twins of the `opensocial` commands
  ([MCP](mcp.md)).

## Where things live

| What | Where |
| --- | --- |
| The group's identity | its DID document, in the PLC directory, changed with the host's rotation key and any recovery keys ([recovery](recovery.md)) |
| Its public records and files | its repo here |
| Its website | a record in its repo here, served at its address ([sites](sites.md)) |
| Its profile, rules and labeler | its `meta` space, in the group's repo there |
| Its roles, permissions, memberships and spaces | its `members` space, in the group's repo there |
| What members write in its spaces | each member's own repo in the space, on their own PDS |
| Invites | the invitee's inbox, a space of their own on their PDS ([membership](membership.md)) |
| Join requests, the moderation queue, tokens, the index | the host's database |
| What it logged, and how it is used | files beside its database, for its operators ([deploying](../deploying.md#checking-it-is-up)) |

## The pieces

| Crate or directory | What it is |
| --- | --- |
| `atproto`, from [its own repository](https://knotless.dev/chadtmiller.com/atproto) | DRISL and CIDs, keys, the MST, CAR files, PLC operations, service auth, JOSE, lexicon data validation, and the space alpha's set hash, commits and tokens, checked against atproto's interop test vectors and its reference implementation, and published to that repository's Cargo registry |
| `crates/group-host` | the server, over SQLite: accounts and repos, spaces, membership and roles, OAuth, moderation, moving, and the console's queries |
| `crates/group-host-mcp` | the MCP tools, each the twin of an `opensocial` command, and what each call comes to, carried out by the host as the member who connected an MCP client and by the command as the person running it |
| `crates/opensocial` | a client for group hosts, and the `opensocial` command ([commands](../reference/cli.md)) |
| `web/` | the console, a SvelteKit app where people start and run groups and approve apps |
| `Dockerfile`, `deploy/` | one image of the host, the console and the command, and the compose file that runs it ([deploying](../deploying.md)) |
| `lexicons/` | the schemas of the records and methods: every `fyi.opensocial.*` method's parameters and input are checked against its own before its handler runs, and a record of our own collections an app writes as a group before it is kept |

## The pages

- [Spaces](spaces.md): what a space is, who reads and writes, credentials,
  and the index.
- [Groups](groups.md): starting a group, what it shows, its profile,
  spaces and handle, and ending it.
- [Membership](membership.md): roles, joining, invites, and leaving.
- [Signing in](signing-in.md): people, apps and MCP clients, and what a
  token may do.
- [Moderation](moderation.md): labels, the queue, and reports.
- [Sites](sites.md): a group's website, published whole and served
  sandboxed at its address or on its own domain, and the live data a
  visitor sees once signed in on it.
- [MCP](mcp.md): connecting an AI app to a group, and its tools.
- [Recovery](recovery.md): keys that let a group's stewards hold its
  identity.
- [Moving](moving.md): a group to another host, an account here as a
  group, and the host changing its address.

The reference lists every [route and method](../reference/api.md),
[MCP tool](../reference/mcp.md), [command](../reference/cli.md) and
[setting](../reference/config.md). [Development](../development.md) says
how to build and test it.
