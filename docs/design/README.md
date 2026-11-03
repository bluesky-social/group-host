# Design

The group host is one server. It is the PDS for every group it hosts, the
authority for their private spaces, their OAuth server, and the web server
for the console. Members keep their own accounts on their own PDSes,
anywhere on the network. opensocial's `docs/design/README.md` explains why
a group needs each of those; this host is that one, written again.

## Why again, and why this way

The TypeScript host is built on `@atproto/*` packages, an embedded
reference PDS and the space alpha's libraries among them, and on Express
and React. This one is built as knotless is: Rust for the server, with
the atproto it needs written for it and held to a 100% coverage gate;
SvelteKit for the console, its types made from the lexicons; and the
lexicons checked against the style guide and against what is published.

It stays wire-compatible. The `fyi.opensocial.*` methods answer as their
lexicons say, so the sites, the templates and apps written against
opensocial.fyi work against it unchanged, and a group can move between
the two hosts with `migrate*`.

## The pieces

| Crate or directory | What it is |
| --- | --- |
| `crates/atproto` | DRISL and CIDs, keys, the MST, CAR files, PLC operations, service auth |
| `crates/group-host` | the server: accounts and repos, spaces, membership and roles, OAuth, moderation, and the console's queries |
| `crates/group-host-mcp` | the core's methods as MCP tools, asked as the member who connected it |
| `crates/opensocial` | a client for group hosts, and the `opensocial` command |
| `web/` | the console, where people start and run groups and approve apps |
| `lexicons/` | the schemas, as opensocial publishes them |

What the TypeScript host's modules bring (the sites at each group's
address, templates, drafts, the network index, notifications) is not part
of this port. The core imports nothing of them there; here, they would
come as crates beside `group-host`, following it rather than called by it.

## The order

Each step is a pull, or a few, with its tests ported first. A step lists
the TypeScript it replaces, under `packages/group-host/src` unless it
says otherwise.

1. **Scaffold.** The workspace, the console's shell, the lexicons, the
   checks. The host answers `_health` and refuses every other method as
   not implemented.
2. **atproto primitives.** DRISL encoding and CIDs, secp256k1 and P-256
   keys and `did:key`, the MST, CAR reading and writing, signed commits,
   PLC operations, service-auth JWTs: what `@atproto/repo`,
   `@atproto/crypto`, `@atproto/identity` and `@did-plc/lib` give it.
   Checked against atproto's interop test vectors.
3. **Accounts and repos.** The SQLite store, group accounts and their
   keys, `did:plc` creation, records and commits, the firehose
   (`subscribeRepos`), blobs, `sync.*`, `did:web` for the host, crawlers:
   `pds/*`, `db/*`, `didWeb.ts`, `actors.ts`.
4. **Groups and spaces.** `createGroup`, `getGroup`, `listGroups`,
   handles, profile, rules, activating and deleting; the meta and members
   spaces, their `access` records, space credentials and the space index:
   `group.ts`, `bootstrap.ts`, `methods/{groups,handle,profile,rules,lifecycle,spaces,spaceRecords}.ts`,
   `spaceIndex.ts`, `methods/checkUserAccess.ts`.
5. **Membership and roles.** Roles and permissions, assigning, joining
   and join requests, invites, leaving and ejecting: `rbac.ts`,
   `methods/{roles,membership,join,invites}.ts`.
6. **Signing in.** Service auth for callers; the OAuth server an app signs
   in to as a group (PAR, DPoP, consent, refresh), group tokens, app
   passwords, connected apps, provisioning: `auth.ts`, `pds/auth.ts`,
   `methods/{authorization,groupAuth,appPasswords,connectedApps,provision}.ts`.
   The console signs its people in at their own PDS here, as a
   confidential client, and keeps the session on the server, as knotless
   does.
7. **The console.** Home, starting a group, a group's page (members,
   roles, invites, requests, apps), approving an app, a person's
   sessions and app passwords: the console's pages in `apps/web/src/pages`.
8. **Moderation, recovery and moving.** Reports, labels, subjects and
   history; recovery keys; importing an account; migrating a group to
   another host and moving the host itself:
   `methods/{moderation,recovery,importAccount,migration}.ts`,
   `hostLabels.ts`, `scripts/move-host.ts`.
9. **MCP and the command.** The MCP tools over what steps 4 to 8 brought,
   and the `opensocial` command's `create-group`, `inspect`, `membership`,
   `migrate`, `move-host` and seeds, as `packages/cli` has them.

The MCP tools and the command's subcommands may come in with each step
instead, beside the methods they call, where that is the smaller pull.
