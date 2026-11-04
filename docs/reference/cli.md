# The commands and the recipes

## `group-host`

| Command | What it does |
| --- | --- |
| none | serves, as the environment says ([config](config.md)) |
| `move-host --from <old address> [--dry-run]` | points each account's DID document at the address the environment names now ([moving the host](../design/moving.md#moving-the-host)) |

Anything else fails with its usage.

## `opensocial`

It acts as a person: it signs in to their own PDS and asks it for a
service token for each method it calls, as an app does, so an account on
any PDS may run it against any host.

| Command | What it does |
| --- | --- |
| `help`, `--help`, `-h`, or none | prints its usage |
| `create-group <handle> [--name] [--description] [--join open\|approval\|invite]` | starts a group, run by you |
| `inspect <group>` | the group as you may see it: its roles, members and spaces |
| `membership assign <group> <member> <role,role…>` | gives a member these roles, in place of theirs (`member` among them) |
| `membership eject <group> <member> [--reason]` | puts a member out |
| `membership invite <group> <person> [--message]` | invites someone in |
| `migrate <group> --to <address> [--to-did <did>]` | moves a group, whole, from this host to another ([moving](../design/moving.md)) |
| `seed <handle>` | a group to try things on: two rules, a moderator role, and an invite for each of `OPENSOCIAL_SEED_INVITE` |

A group or person is a handle or a DID. Anything else fails with its
usage.

| Variable | What it is |
| --- | --- |
| `OPENSOCIAL_AS`, `OPENSOCIAL_PASSWORD` | the handle you sign in as, and its password (an app password is enough) |
| `PDS_URL` | where you sign in; `http://localhost:2583` |
| `OPENSOCIAL_HOST` | the host; `http://localhost:2600` |
| `OPENSOCIAL_HOST_DID` | its DID; the did:web of its address |
| `OPENSOCIAL_SEED_INVITE` | who `seed` invites, comma-separated |

## `just`

| Recipe | What it does |
| --- | --- |
| `setup` | installs the console's packages and cargo-llvm-cov |
| `test`, `t <args>` | every test, or those `args` pick |
| `cov`, `cov-html` | the tests under the 100% gate, or their coverage as a page |
| `lint` | formatting, clippy, the console's layers and svelte-check |
| `lex` | the lexicons' checks |
| `serve` | the host |
| `web`, `web-dev` | the console built, or served as its source changes |
| `e2e <args>` | the console's flows in Chromium over the reference PDS, as the `e2e` check runs them |
| `opensocial <args>` | the command |
