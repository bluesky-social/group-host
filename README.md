# group-host

> **Alpha.** group-host is early and changes often. Its `org.atgrp.*`
> lexicons are not yet published and will change, as will the space
> alpha it builds on, so apps and records written against today's will
> break. Don't run it for a group you can't afford to lose.

group-host is a group host for atproto. A group on it is an account of
its own, with no password and no one person behind it. The host is the
group's PDS, the authority for its private spaces and its OAuth server,
and its members' roles decide what anyone may do there. Members keep
their own accounts on their own PDSes, anywhere on the network. It
implements the [Atmosphere Groups proposal](https://tangled.org/opensocial.group/proposal).

It's one Rust server over SQLite, with a console (a SvelteKit app) where
people start and run groups and approve apps, and a command,
`atgrp`, that does the same from a terminal.

## Highlights

- **Groups are accounts.** Each group has its own `did:plc`, public repo
  and handle, on the host's domain or one of its own. Its profile, rules,
  roles and memberships are records it keeps in its spaces.
  → [Groups](docs/design/groups.md)
- **Private spaces.** A space is a private record store. The group and
  each member write in their own repos there, members on their own PDSes,
  and the host decides who reads and writes, gives out credentials and
  indexes what was written. → [Spaces](docs/design/spaces.md)
- **Roles decide.** A role grants actions, from inviting to moderating.
  People join openly, by approval or by invite, and an invite lands in
  their own inbox. Moderators label, work a queue and answer reports and
  appeals. → [Membership](docs/design/membership.md),
  [Moderation](docs/design/moderation.md)
- **Apps sign in as the group.** A member approves an app over OAuth,
  and it gets what that member's roles allow, checked again at every
  write. → [Signing in](docs/design/signing-in.md)
- **One command, one tool.** An `atgrp` command acts in a group
  as you, and its MCP tool twin acts the same way for a member through an
  AI app they connect. Only the commands that make, move or end a group
  or an account have no tool. → [MCP](docs/design/mcp.md),
  [commands](docs/reference/cli.md)
- **The group can leave.** Stewards hold recovery keys that rank above
  the host's own, so they can undo what the host does to the group's
  identity. A group moves to another host with its repo, files and
  spaces. → [Recovery](docs/design/recovery.md),
  [Moving](docs/design/moving.md)

## Quickstart

You need Rust's stable toolchain through rustup, Node at the version
`.node-version` names, `just`, and goat (github.com/bluesky-social/goat)
for the lexicons.

```sh
just setup     # the console's and harness's packages, and coverage tools
just dev       # a PDS, a PLC directory, the host and the console, with two groups
```

`just dev` serves the console at https://group-host.localtest.me. Sign
in as `alice.test` with the password `password`. She runs a group,
Riders, and has an invite waiting from bob's group, Hikers. `just dev`
prints the rest: the other accounts, where to read the codes the PDS
emails, and how to run the command against it, such as:

```sh
ATGRP_HOST=… PDS_URL=… ATGRP_AS=alice.test ATGRP_PASSWORD=password \
  just atgrp group show riders.groups.localtest.me
```

[Development](docs/development.md) has the rest of the dev world, and
`just serve` and `just web-dev` for the host and the console alone.

To deploy, build the image and run it with `deploy/docker-compose.yml`
behind a reverse proxy, as [Deploying](docs/deploying.md) sets out:

```sh
docker build --build-arg GROUP_HOST_COMMIT=$(git rev-parse HEAD) -t group-host:current .
docker compose up -d
```

## Docs

The same pages are a site, made by `docs-site/`.

| | |
| --- | --- |
| [Guides](docs/guides/README.md) | Doing each thing in the console, step by step, with a picture of each screen |
| [Design](docs/design/README.md) | What the host is to a group, where each kind of data lives, and why each part works the way it does |
| [API](docs/reference/api.md) | Every HTTP route and XRPC method, and the errors each answers |
| [Commands](docs/reference/cli.md) | Every `atgrp` command and `just` recipe |
| [MCP tools](docs/reference/mcp.md) | Every tool, and the command it's the twin of |
| [Configuration](docs/reference/config.md) | Every environment variable the host reads |
| [Deploying](docs/deploying.md) | The image, running it behind a proxy, and what to back up |
| [Development](docs/development.md) | Building, testing, the checks a pull runs, and the docs site |

## The pieces

The server is `crates/group-host`, the command `crates/atgrp`, the
MCP tools `crates/group-host-mcp` and the console `web/`. The atproto
primitives are the `atproto` crate, in
[its own repository](https://knotless.dev/chadtmiller.com/atproto).
[The pieces](docs/design/README.md#the-pieces) says what each one holds.

## Development

```sh
just test      # every test
just t <args>  # the tests those cargo test args pick
just cov       # every test, under the 100% gate of lines and functions
just lint      # formatting, clippy, the console's layers and dialogs, the type checks
just fmt       # rustfmt and oxfmt
just e2e       # the console's flows in Chromium
```

[Development](docs/development.md) says what each check runs. AGENTS.md
is for agents working here.

## License

Licensed under MIT ([LICENSE-MIT](LICENSE-MIT)) or Apache-2.0
([LICENSE-APACHE](LICENSE-APACHE)), at your option.
