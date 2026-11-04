# The commands and the recipes

## `group-host`

| Command | What it does |
| --- | --- |
| none | serves, as the environment says ([config](config.md)) |
| `move-host --from <old address> [--dry-run]` | run with the host stopped and the environment naming the new address: points each account's DID document there, printing a line for each, and tells the relays; run again, it repoints nothing and tells them again ([moving the host](../design/moving.md#when-the-host-changes-its-address)) |

Anything else fails with its usage.

## `opensocial`

It acts as a person: it signs in to their own PDS and asks it for a
service token for each method it calls, as an app does, so an account on
any PDS may run it against any host.

| Command | What it does |
| --- | --- |
| `help`, `--help`, `-h`, or none | prints its usage |
| `create-group <handle> [--name] [--description] [--join open\|approval\|invite]` | starts a group, run by you; `--name` is the handle's first label if not given, and the join policy `approval` |
| `inspect <group>` | the group as you may see it: its roles, members and spaces |
| `membership assign <group> <member> <role,role…>` | gives a member these roles, in place of theirs (`member` among them) |
| `membership eject <group> <member> [--reason]` | puts a member out |
| `membership invite <group> <person> [--message]` | invites someone in |
| `migrate <group> --to <address> [--to-did <did>]` | moves a group, whole, from this host to another, making the four calls in turn; run again after a run that handed it over but could not switch it on, it switches it on; `--to-did` is the did:web of `--to` if not given ([moving](../design/moving.md)) |
| `import <account> [--address <label>]` | moves an account from its server here, its spaces with it, to become a group run by you; its server emails a code ([moving](../design/moving.md#bringing-an-account-here-as-a-group)) |
| `import <account> --code <code>` | with that code, has its server sign its identity over, and finishes, whatever `--address` the start was given |
| `seed <handle>` | a group to try things on, joined by approval: two rules, a moderator role (`mod.read`, `mod.resolve`, `label`), and an invite for each of `OPENSOCIAL_SEED_INVITE` |

A group or person is a handle or a DID, resolved by the host's
`resolveHandle` as you. An unknown command or flag fails with its usage;
any failure exits 1.

| Variable | What it is |
| --- | --- |
| `OPENSOCIAL_AS`, `OPENSOCIAL_PASSWORD` | the handle you sign in as, and its password (an app password is enough) |
| `PDS_URL` | where you sign in; `http://localhost:2583` |
| `OPENSOCIAL_HOST` | the host; `http://localhost:2600` |
| `OPENSOCIAL_HOST_DID` | its DID; the did:web of its address |
| `OPENSOCIAL_SEED_INVITE` | who `seed` invites, comma-separated |
| `OPENSOCIAL_IMPORT_PASSWORD` | the password of the account `import` moves, at its own server: its own, as its server signs its identity over for no app password |

## `just`

| Recipe | What it does |
| --- | --- |
| `setup` | installs the console's and the harness's packages, llvm-tools and cargo-llvm-cov |
| `test`, `t <args>` | every test, or those `args` pick |
| `cov`, `cov-html` | the tests under the 100% gate, or their coverage as a page |
| `lint` | formatting, clippy, the console's layers and svelte-check |
| `lex` | the lexicons' checks |
| `dev` | the console to use, over a world of its own: the reference PDS with alice, bob and harbor, the host, and the console at https://group-host.localtest.me ([development](../development.md#using-it)) |
| `serve` | the host |
| `web`, `web-dev` | the console built, or served as its source changes |
| `e2e <args>` | builds the console and the host, then runs the console's flows in Chromium over the reference PDS, as the `e2e` check does |
| `opensocial <args>` | the command |
| `docs` | the docs' links and anchors, as the `docs` check runs it |
| `docs-site` | the docs as their site, at http://localhost:4321/, as they change |
| `docs-deploy` | the docs site, built and deployed as the repository's ([development](../development.md#the-docs-site)) |
