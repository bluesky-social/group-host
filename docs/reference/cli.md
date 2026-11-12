# The commands and the recipes

## `group-host`

| Command | What it does |
| --- | --- |
| none | serves, as the environment says ([config](config.md)) |
| `move-host --from <old address> [--dry-run]` | run with the host stopped and the environment naming the new address: points each account's DID document there, printing a line for each, and tells the relays; an account the directory fails on is reported, the rest moved, and the command exits with an error; run again, it repoints nothing and tells them again ([moving the host](../design/moving.md#when-the-host-changes-its-address)) |

Anything else fails with its usage.

## `opensocial`

It acts as a person: it signs in to their own PDS and asks it for a
service token for each method it calls, as an app does, so an account on
any PDS may run it against any host. To write as a group, as an invite,
a rule or a membership is written, it takes a group token from the host
for that space (`getGroupAuth`).

Every command is `opensocial <group> <action>`, then the group it acts
in, a handle or a DID resolved by the host's `resolveHandle` as you,
then the arguments the command takes in order, then its flags. The
groups are singular nouns for what they act on: `group`, `profile`,
`rule`, `role`, `member`, `join`, `invite`, `space`, `mod`,
`account` and `admin`, which acts on the host rather than a group. An action with an opposite takes `--undo` (`invite send
--undo`). `help`, `--help`, `-h` or nothing prints the usage, by group.

Each command that acts in a group is the twin of an MCP tool,
`<group>_<action>` ([MCP tools](mcp.md)), and reads its words by that
tool's schema: a flag is a field of what it writes or asks, in kebab
case (`--display-name` for `displayName`); one that is yes or no is
`--name` or `--no-name`; a number is a number; a list is
comma-separated (`member,admin`); anything more than that is JSON
(`--access '{"grants": []}'`). A person (`<member>`, `<person>`) may be
a handle, which it resolves. Such a command prints what the host
answered, as JSON, but `group show`, which prints lines. A write
replaces the record whole, a field not given dropped, but `profile set`,
which changes only the fields given (`""` takes one away), and `role
grant`, which adds to a role's grant or with `--undo` takes from it.

The commands with no tool make, move or delete a group or an account:
`group create`, `group seed`, `group move`, `group delete` and `account
import`. A client connected to a group can switch it off, which its
stewards undo, but never delete it. Nor have `admin logs` and `admin
stats`, which read the whole host, every group's requests in it, while
an MCP client acts in one group.

### group

| Command | What it does |
| --- | --- |
| `group create <handle> [--display-name] [--description] [--join-policy open\|approval\|invite]` | starts a group, run by you; its name is the handle's first label if not given, and its join policy `approval` |
| `group show <group>` | the group as you may see it: its roles, its first hundred members and its spaces |
| `group seed <handle>` | a group to try things on, joined by approval: two rules, a moderator role (`mod.read`, `mod.resolve`, `label`) that the founder may give, written as the group, and an invite for each of `OPENSOCIAL_SEED_INVITE` |
| `group move <group> --to <address> [--to-did <did>]` | moves a group, whole, from this host to another, making the four calls in turn; run again after a run that handed it over but could not switch it on, it switches it on; `--to-did` is the did:web of `--to` if not given ([moving](../design/moving.md)) |
| `group deactivate <group> [--undo]` | switches the group off, so nothing reads its spaces or writes as it; with `--undo`, switches it back on, for one who held `group.configure` when it went off ([switching off](../design/groups.md#switching-off-on-and-deleting)) |
| `group handle <group> <handle>` | gives the group a handle on a domain of its own, once the domain names it, or its own address back; refused with `NotPointing`, saying what the domain's `_atproto` record should be, until it does ([a handle on its own domain](../design/groups.md#a-handle-on-its-own-domain)) |
| `group delete <group>` | deletes the group, its spaces and its account here, for good, and prints whether its DID was ended or left to the holders of its recovery keys |

### profile

| Command | What it does |
| --- | --- |
| `profile set <group> [--display-name <name>] [--join-policy <policy>] [--description] [--url] [--online] [--topics <json>] [--languages <json>] [--location <json>] [--links <json>]` | changes the group's profile in the fields given, as the group, keeping the rest and its pictures; a field given `""` is taken away; a new link is given its site's icon, from the favicon service `FAVICONS` names |

### rule

| Command | What it does |
| --- | --- |
| `rule set <group> --title <title> --text <words> [--order <n>] [--rule <key>]` | writes a rule as the group: the one `--rule` names, or a new one |
| `rule delete <group> <rule>` | deletes a rule, by its key |

### label

| Command | What it does |
| --- | --- |
| `label define <group> <label> --name <name> --description <words> [--lang <lang>] [--severity inform\|alert\|none] [--blurs content\|media\|none] [--default-setting ignore\|warn\|hide] [--adult-only]` | defines one of the group's own labels, lowercase letters and hyphens, or changes one it defines: what it is called and means, and how apps should show what wears it |
| `label delete <group> <label>` | stops defining one of the group's own labels; labels already applied stay |

### role

| Command | What it does |
| --- | --- |
| `role set <group> <role> --display-name <name> [--description] [--default] [--bot]` | writes a role as the group, at its id |
| `role delete <group> <role>` | deletes a role nobody holds or is granted |
| `role grant <group> <role> [--scopes <scope,…>] [--assignable <role,…>] [--undo]` | adds the scopes given, and the roles given that its holders may assign, to the role's grant in the permissions record; with `--undo`, takes them away, or with none given drops its grant |

### site

| Command | What it does |
| --- | --- |
| `site publish <group> <dir> [--fallback <path>] [--not-found <path>]` | publishes the files under `<dir>`, but those whose names start with a dot, as the group's website at its address, in place of the one there: each as text if it is UTF-8, else as bytes |
| `site delete <group>` | takes the group's website down |

### member

| Command | What it does |
| --- | --- |
| `member list <group> [--q <text>] [--limit <n>] [--cursor <cursor>]` | who is in the group, and their roles, a page at a time; with `--q`, those whose handle or DID holds it |
| `member assign <group> <member> <role,role…>` | writes their membership as the group: lets in someone waiting to join, or gives a member these roles in place of theirs (`member` among them) |
| `member eject <group> <member>` | deletes their membership as the group, putting them out |

### join

| Command | What it does |
| --- | --- |
| `join list <group> [--limit <n>] [--cursor <cursor>]` | the requests to join waiting |
| `join deny <group> <member>` | turns down a request to join |

### invite

| Command | What it does |
| --- | --- |
| `invite send <group> <person> [--message <words>] [--expires-at <time>] [--undo]` | invites someone in, as the group: an invite written in their inbox; with `--undo`, takes back the one waiting for them |
| `invite list <group> [--limit <n>] [--cursor <cursor>]` | the invites waiting |

### space

| Command | What it does |
| --- | --- |
| `space create <group> <type> --access <json> [--name] [--skey] [--no-index]` | adds a space to the group |
| `space records <group> <space> [--collection] [--field <path> --equals <value>] [--uri] [--limit] [--cursor]` | the records in one of the group's spaces |

### mod

A subject is an account's DID or a record's `at://` URI, with `--cid`
where a version is asked for, as a label on a record asks.

| Command | What it does |
| --- | --- |
| `mod list <group> [--status open\|escalated\|resolved] [--limit] [--cursor]` | the moderation queue |
| `mod history <group> <subject> [--cid] [--limit] [--cursor]` | what was done about a subject |
| `mod resolve <group> <subject> --status resolved\|escalated [--note] [--cid]` | resolves or escalates a subject |
| `mod label <group> <subject> <val> [--cid] [--undo]` | labels a subject as the group, with a label record beside it; with `--undo`, deletes it |

### account

| Command | What it does |
| --- | --- |
| `account import <account> [--address <label>]` | moves an account from its server here, its spaces with it, to become a group run by you; its server emails a code ([moving](../design/moving.md#bringing-an-account-here-as-a-group)) |
| `account import <account> --code <code>` | with that code, has its server sign its identity over, and finishes, whatever `--address` the start was given |

### admin

For the host's operators, the accounts its `HOST_ADMINS` names
([config](config.md)); anyone else is refused `Forbidden`.

| Command | What it does |
| --- | --- |
| `admin logs [--trace <id>] [--message <message>] [--level <level>] [--min-ms <n>] [--limit <n>]` | the lines the host logged, oldest first, each as JSON on a line of its own, as it wrote them: every line of one request, by the `x-trace-id` it was answered with (500 at most), or the last 100 (or `--limit`, up to 500) of a message (`request`, `mcp tool`), at a level or above (`error`, `warn`, `info`, `debug`, `trace`), or that took `--min-ms` or longer (`getLogs`) |
| `admin stats [--days <n>]` | how the host is used, over the last week or `--days` (up to 400), today the last, in UTC: what it holds (groups, those switched off, spaces, bytes of files); a row for each day (requests, refused with a 4xx, failed with a 5xx, slow at a second or more, MCP tool calls, groups and spaces made); then each route asked, the most asked first, and each tool called (`getStats`) |

An unknown command or flag fails with its usage; any failure exits 1.

| Variable | What it is |
| --- | --- |
| `OPENSOCIAL_AS`, `OPENSOCIAL_PASSWORD` | the handle you sign in as, and its password (an app password is enough) |
| `PDS_URL` | where you sign in; `http://localhost:2583` |
| `OPENSOCIAL_HOST` | the host; `http://localhost:2600` |
| `OPENSOCIAL_HOST_DID` | its DID; the did:web of its address |
| `OPENSOCIAL_SEED_INVITE` | who `group seed` invites, comma-separated |
| `OPENSOCIAL_IMPORT_PASSWORD` | the password of the account `account import` moves, at its own server: its own, as its server signs its identity over for no app password |

## `just`

| Recipe | What it does |
| --- | --- |
| `setup` | installs the console's and the harness's packages, llvm-tools and cargo-llvm-cov |
| `test`, `t <args>` | every test, or those `args` pick |
| `cov`, `cov-html` | the tests under the 100% gate, or their coverage as a page |
| `fmt` | formats the Rust with rustfmt, and the console, the harness and the docs site with oxfmt |
| `lint` | formatting, clippy, the console's layers and svelte-check |
| `lex` | the lexicons' checks |
| `dev` | the console to use, over a world of its own: the reference PDS with alice, bob and harbor, the host, the console at https://group-host.localtest.me, and an invite waiting for alice ([development](../development.md#using-it)) |
| `serve` | the host |
| `web`, `web-dev` | the console built, or served as its source changes |
| `look <path>… [options]` | draws the console's pages as pictures in `target/look`, in any state: `--as <account>`, `--seed <name\|file>`, `--click`, `--type`, `--element`, `--phone`, `--desktop`, `--light`, `--dark`, `--all` ([development](../development.md#looking-at-the-console)) |
| `look-up` | a world for `look` to draw over, kept until ctrl-c |
| `shots [--check] [guide]…` | draws the guides' pictures and their text again into `docs/guides/shots`, from the look line above each, each guide over a fresh world; with `--check`, writes nothing and fails if a picture's text has changed, as the `e2e` check does ([development](../development.md#the-guides-pictures)) |
| `e2e <args>` | builds the console and the host, then runs the console's flows in Chromium over the reference PDS, as the `e2e` check does |
| `opensocial <args>` | the command |
| `docs` | the docs' links and anchors, as the `docs` check runs it |
| `docs-site` | the docs as their site, at http://localhost:4321/, as they change |
| `docs-deploy` | the docs site, built and deployed as the repository's ([development](../development.md#the-docs-site)) |
