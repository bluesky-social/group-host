# group-host

> **Beta.** `group-host` follows the Atmosphere Groups spec, which is in
> beta and may still change. The
> [atproto spaces](https://github.com/bluesky-social/proposals/tree/main/0016-permissioned-data)
> it builds on are in beta too.

`group-host` is a group server for [atproto](https://atproto.com) that
implements the
[Atmosphere Groups spec](https://tangled.org/atmospheregroups.org/spec).
A group on the host is an atproto account of its own, with no password
and no single person behind it. This host is each group's PDS (the
server that holds its public repo and blobs), the authority for its
private spaces, and its OAuth server. Members keep their own accounts on
their own PDSes anywhere on the network, and their roles in a group
decide what anyone may do there.

The host is one Rust server (`crates/group-host`) over SQLite. Beside it
sit a SvelteKit console (`web/`) where people start and run groups and
approve apps, and the `atgrp` command (`crates/atgrp`) that does the
same from a terminal.

## Highlights

- **Groups are accounts.** Each group gets its own `did:plc` identity,
  public repo, and blobs on the host, with a handle under the host's
  `HANDLE_DOMAIN` or on its own domain. It keeps its profile, settings,
  rules, roles, memberships, labeler, and space list as `org.atgrp.*`
  records in its own `meta` and `members` spaces. →
  [Groups](docs/design/groups.md)
- **Private spaces.** A space is a private record store from atproto's
  permissioned data, where the group writes in its repo on this
  host and each member writes in their own repo on their own PDS. The
  host acts as the space's authority, checking reads and writes against
  the space's `org.atgrp.policy` and the member's roles. It issues read
  credentials and indexes every writer's records so readers can query
  the space. → [Spaces](docs/design/spaces.md)
- **Roles, membership, and moderation.** Roles (`org.atgrp.role`) grant
  permissions across a group and its spaces, from inviting and admitting
  members to moderating. People join openly, by approval, or through an
  `org.atgrp.invite` record written into their own `org.atgrp.inbox`
  space on their PDS. Moderators apply and remove
  `com.atproto.label.record` labels, work a moderation queue, and handle
  reports and appeals. → [Membership](docs/design/membership.md),
  [Moderation](docs/design/moderation.md)
- **Apps sign in as the group.** A member approves an OAuth client on
  the console, granting it at most what that member's roles allow. The
  host binds tokens with DPoP and checks the member's current roles
  again on every write, so a role change takes effect right away. →
  [Signing in](docs/design/signing-in.md)
- **One command, one MCP tool.** An `atgrp <group> <action>` command
  acts in a group as the person running it, and its MCP (Model Context
  Protocol) tool twin `<group>_<action>` at `/mcp` carries out the same
  plan for an AI app a member connects to that group. Only the commands
  that create, seed, move, or delete a group or import an account, plus
  the host-wide `admin` commands, have no tool. →
  [MCP](docs/design/mcp.md), [Commands](docs/reference/cli.md)
- **Recovery and moving.** Stewards (members trusted with the group's
  identity) can add up to four recovery keys that rank above the host's
  rotation key on the group's `did:plc` document, letting them undo PLC
  operations the host signed and move the group without the host. A
  group can move to another host with its DID, public repo, files, and
  spaces. An existing `did:plc` account can also move here to become a
  group, and the host can change its own address. →
  [Recovery](docs/design/recovery.md), [Moving](docs/design/moving.md)
- **Rate limits.** The host caps how much a caller can ask in fixed time
  windows of points counted by client IP (or IPv6 `/64`), by group, or
  by signed-in person. When a caller runs out of points, the host
  answers `429 RateLimitExceeded` with `Retry-After` and `RateLimit-*`
  headers. → [Limits](docs/design/limits.md)

## Quickstart

You need Rust's stable toolchain through `rustup`, Node at the version
in `.node-version`, `just`, and `goat` (for linting lexicons). From a
fresh checkout, install the packages and the tools the checks use, then
start a local world:

```sh
just setup
just dev
```

`just dev` builds the lexicon types, `group-host`, and `atgrp`, then
starts a reference PDS, a PLC directory, the host, and the console at
[https://group-host.localtest.me](https://group-host.localtest.me)
(behind `portless` on port 443). If `portless` isn't running yet, `just
dev` prints the command to start it. You can sign in as `alice.test` or
`bob.test` with the password `password`, and `just dev` prints a `just
atgrp` command to run as `alice.test` against her group. To run the
processes on their own, use `just serve` (on `PORT`, `2600` by default)
and `just web-dev`.

To deploy, build the single container image and run the `host` and
`console` services in `deploy/docker-compose.yml` behind a reverse
proxy:

```sh
docker build --build-arg GROUP_HOST_COMMIT=$(git rev-parse HEAD) -t group-host:current .
docker compose -f deploy/docker-compose.yml up -d
```

See [Deploying](docs/deploying.md) for release images, reverse proxy
setup, custom domains, and backups.

## Docs

The pages under `docs/` (also built into a Starlight site by
`docs-site/`):

| Page | What's in it |
|---|---|
| [Guides](docs/guides/README.md) | Step-by-step console walkthroughs with light and dark screenshots |
| [Design](docs/design/README.md) | What the host is to a group, where data lives, and how each mechanism works |
| [API](docs/reference/api.md) | Every HTTP route and XRPC method, and the errors each returns |
| [Commands](docs/reference/cli.md) | Every `atgrp` command, `group-host` command, and `just` recipe |
| [MCP tools](docs/reference/mcp.md) | Every MCP tool and the `atgrp` command it matches |
| [Configuration](docs/reference/config.md) | Every environment variable the host and `atgrp` read |
| [Deploying](docs/deploying.md) | Building or pulling the image, reverse proxy setup, custom domains, upgrades, and backups |
| [Development](docs/development.md) | Building, testing, pull checks, the docs site, and releasing |

## The pieces

The workspace holds the server in `crates/group-host`, the CLI and
client in `crates/atgrp`, the MCP tools in `crates/group-host-mcp`, the
SvelteKit console in `web/`, the schemas in `lexicons/`, the container
and release files in `Dockerfile` and `deploy/`, the dev and test
harness in `harness/`, and the Starlight site in `docs-site/`. The
`atproto` and `atproto-client` crates live in
[their own repository](https://knotless.dev/chadtmiller.com/atproto),
and the full table is in
[The pieces](docs/design/README.md#the-pieces).

## Development

```sh
just test     # every workspace test
just t <args> # only the tests those cargo test args pick
just cov      # 100% line and function coverage gate
just lint     # rustfmt, oxfmt, clippy, cargo-deny, console checks, svelte-check
just fmt      # format Rust with rustfmt and web/harness/docs-site with oxfmt
just lex      # lint lexicons with goat and check growth against main
just docs     # check relative links, #anchor slugs, and guide screenshots
just e2e      # run the console's end-to-end flows in Chromium with Playwright
```

See [Development](docs/development.md) for what each check runs, and
`AGENTS.md` when working on the repository with a coding agent.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT), at your option.

