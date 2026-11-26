# The commands and the recipes

## `group-host`

| Command                                      | What it does                                                                                                                                                                                                                                                                                                                                                                                                      |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| none                                         | serves, as the environment says ([config](config.md))                                                                                                                                                                                                                                                                                                                                                             |
| `move-host --from <old address> [--dry-run]` | run it with the host stopped and the environment naming the new address. It points each account's DID document there, printing a line for each, and tells the relays. If the directory fails on an account, it reports that account, moves the rest and exits with an error. Run again, it repoints nothing and tells the relays again ([moving the host](../design/moving.md#when-the-host-changes-its-address)) |

Anything else fails with its usage.

## `atgrp`

It acts as a person. It signs in to your own PDS and asks it for a
service token for each method it calls, as an app does, so an account on
any PDS can run it against any host. To write as a group (an invite, a
rule or a membership), it takes a group token from the host for that
space (`getGroupAuth`).

Every command is `atgrp <group> <action>`, followed by the group it
acts in, then the arguments the command takes in order, then its flags.
The group is a handle or a DID, which the host's `resolveHandle`
resolves as you. The command groups are singular nouns for what they act
on: `group`, `profile`, `rule`, `role`, `member`, `join`, `invite`,
`space`, `mod`, `account` and `admin`. `admin` acts on the host instead
of a group. An action with an opposite takes `--undo` (`invite send
--undo`). `help`, `--help`, `-h` or nothing prints the usage, by group.

Each command that acts in a group is the twin of an MCP tool,
`<group>_<action>` ([MCP tools](mcp.md)), and reads its words by that
tool's schema. A flag is a field of what it writes or asks, in kebab
case (`--display-name` for `displayName`). A yes-or-no field is
`--name` or `--no-name`, and a number is a number. A list is
comma-separated (`member,admin`), and anything more is JSON
(`--access '{"grants": []}'`). A person (`<member>`, `<person>`) can be
a handle, which the command resolves. These commands print what the host
answered as JSON, except `group show`, which prints lines. A write
replaces the record whole and drops any field not given. The exceptions
are `profile set`, which changes only the fields given (`""` removes
one), and `role grant`, which adds to what a role's record grants, or
takes from it with `--undo`.

The commands with no tool make, move or delete a group or an account:
`group create`, `group seed`, `group move`, `group delete` and `account
import`. A client connected to a group can switch it off, and its
stewards can undo that, but it can't delete the group. `admin logs` and
`admin stats` have no tool either. They read the whole host, with every
group's requests in it, while an MCP client acts in one group.

### group

| Command                                                                                         | What it does                                                                                                                                                                                                                                                                                                  |
| ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `group create <handle> [--display-name] [--description] [--join-policy open\|approval\|invite]` | starts a group that you run. Its name defaults to the handle's first label and its join policy to `approval`                                                                                                                                                                                                  |
| `group show <group>`                                                                            | shows the group as you may see it: its roles, its first hundred members and its spaces                                                                                                                                                                                                                        |
| `group seed <handle>`                                                                           | makes a group to try things on, joined by approval. It has two rules, a moderator role (`include:org.atgrp.authModerate`, and labels in every space) that the founder may give, written as the group, and an invite for each of `ATGRP_SEED_INVITE`                                                            |
| `group move <group> --to <address> [--to-did <did>]`                                            | moves a whole group from this host to another, making the four calls in turn. If a run handed the group over but couldn't switch it on, running it again switches it on. `--to-did` defaults to the did:web of `--to` ([moving](../design/moving.md))                                                         |
| `group deactivate <group> [--undo]`                                                             | switches the group off, so nothing reads its spaces or writes as it. With `--undo`, it switches the group back on, for someone who held `group.configure` when it went off ([switching off](../design/groups.md#switching-off-on-and-deleting))                                                               |
| `group handle <group> <handle>`                                                                 | gives the group a handle on a domain of its own once the domain names it, or gives its own address back. Until the domain names it, the host refuses with `NotPointing` and says what the domain's `_atproto` record should be ([a handle on its own domain](../design/groups.md#a-handle-on-its-own-domain)) |
| `group domain <group> [--undo]`                                                                 | serves the group at its handle's own domain once that domain reaches this host, and redirects its address and `www.` there. With `--undo`, it stops ([its own domain](../design/groups.md#its-own-domain))                                                                                                     |
| `group delete <group>`                                                                          | deletes the group, its spaces and its account here for good. It prints whether the DID was ended or left to the holders of its recovery keys                                                                                                                                                                  |

### profile

| Command                                                                                         | What it does                                                                                                                         |
| ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `profile set <group> [--display-name <name>] [--description <words>] [--website <url>] [--avatar <blob json>] [--join-policy open\|approval\|invite]` | changes the fields given in the group's about record (`org.atgrp.about`, at `self` in its meta space), as the group, and keeps the rest. `--join-policy` changes its config record (`org.atgrp.config`, beside it), which is written only when it's given. A field given `""` is removed. It answers `{"written": [...]}`, one write for each record changed |

### rule

| Command                                                                        | What it does                                                           |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| `rule set <group> --title <title> --text <words> [--order <n>] [--rule <key>]` | writes a rule as the group, either the one `--rule` names or a new one |
| `rule delete <group> <rule>`                                                   | deletes a rule by its key                                              |

### label

| Command                                                                                                                                                                                                  | What it does                                                                                                                                                                         |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `label define <group> <label> --name <name> --description <words> [--lang <lang>] [--severity inform\|alert\|none] [--blurs content\|media\|none] [--default-setting ignore\|warn\|hide] [--adult-only]` | defines one of the group's own labels (lowercase letters and hyphens), or changes one it defines. It sets what the label is called and means, and how apps should show what wears it |
| `label delete <group> <label>`                                                                                                                                                                           | stops defining one of the group's own labels. Labels already applied stay                                                                                                            |

### role

| Command | What it does |
| --- | --- |
| `role set <group> <role> [--display-name <name>] [--description] [--default] [--bot] [--permissions <permission,…>] [--can-assign <role,…>]` | writes a role in the members space as the group, at its id (lowercase letters and hyphens, three or more), with the permissions it carries across the group and the roles its holders may assign (`*` for every role) |
| `role delete <group> <role>` | deletes a role nobody holds |
| `role grant <group> <role> [--permissions <permission,…>] [--can-assign <role,…>] [--undo]` | adds the permissions given, and the roles its holders may assign, to those in the role's record. With `--undo`, it takes them away, or takes all of both if none are given. The rest of the record stays as it was |

### member

| Command                                                              | What it does                                                                                                                                     |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `member list <group> [--q <text>] [--limit <n>] [--cursor <cursor>]` | lists who's in the group and their roles, a page at a time. With `--q`, it lists those whose handle or DID contains it                           |
| `member assign <group> <member> <role,role…>`                        | writes their membership as the group. It lets in someone waiting to join, or gives a member these roles in place of theirs (`member` among them) |
| `member eject <group> <member>`                                      | deletes their membership as the group, which puts them out                                                                                       |

### join

| Command                                               | What it does                                |
| ----------------------------------------------------- | ------------------------------------------- |
| `join list <group> [--limit <n>] [--cursor <cursor>]` | lists the requests to join that are waiting |
| `join deny <group> <member>`                          | turns down a request to join                |

### invite

| Command                                                                           | What it does                                                                                                                   |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `invite send <group> <person> [--message <words>] [--expires-at <time>] [--undo]` | invites someone in as the group, by writing an invite in their inbox. With `--undo`, it takes back the invite waiting for them |
| `invite list <group> [--limit <n>] [--cursor <cursor>]`                           | lists the invites that are waiting                                                                                             |

### space

| Command                                                                                                       | What it does                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `space create <group> <type> --policy <json> [--skey <key>]`                                                  | makes a space of `<type>` in the group under the `org.atgrp.policy` given, with `com.atproto.space.createSpace` and a group token for the space. Without `--skey`, it gets a new key |
| `space records <group> <space> [--collection] [--field <path> --equals <value>] [--uri] [--limit] [--cursor]` | lists the records in one of the group's spaces                                                                                                                                            |

### mod

A subject is an account's DID or a record's URI: `at:space/…` in one of
the group's spaces, `at://…` in a public repo. Where a version is asked
for, add `--cid`, the way a label on a record does.

| Command                                                                       | What it does                                                                                      |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `mod list <group> [--status open\|resolved] [--tags <tag,…>] [--sort oldest\|newest\|updated] [--limit] [--cursor]` | shows the moderation queue: every subject, or those of a status or with any of the tags, oldest first unless `--sort` says otherwise (`org.atgrp.mod.querySubjects`) |
| `mod history <group> <subject> [--cid] [--sort newest\|oldest] [--limit] [--cursor]` | shows what was done about a subject, newest first unless `--sort` says otherwise (`org.atgrp.mod.querySubjectHistory`) |
| `mod update <group> <subject> [--status open\|resolved] [--note <words>] [--add-tags <tag,…>] [--remove-tags <tag,…>] [--cid]` | resolves or reopens a subject, adds or removes its tags (lowercase letters and hyphens), or adds a note. What isn't given stays as it was (`org.atgrp.mod.updateSubject`) |
| `mod label <group> <subject> <val> [--cid] [--undo]`                          | labels a subject as the group, with a label record beside it, where the member's roles write labels. With `--undo`, it deletes the label |

### account

| Command                                        | What it does                                                                                                                                                                          |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `account import <account> [--address <label>]` | moves an account and its spaces from its server to here, to become a group that you run. Its server emails a code ([moving](../design/moving.md#bringing-an-account-here-as-a-group)) |
| `account import <account> --code <code>`       | uses that code to have its server sign its identity over, and finishes. It keeps the `--address` the start was given                                                                  |

### admin

These are for the host's operators, the accounts its `HOST_ADMINS` names
([config](config.md)). The host refuses anyone else with `Forbidden`.

| Command                                                                                          | What it does                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `admin logs [--trace <id>] [--message <message>] [--level <level>] [--min-ms <n>] [--limit <n>]` | prints the lines the host logged, oldest first, each as JSON on its own line as the host wrote it. It prints every line of one request by the `x-trace-id` it was answered with (500 at most). Otherwise it prints the last 100 (or `--limit`, up to 500) of a message (`request`, `mcp tool`), at a level or above (`error`, `warn`, `info`, `debug`, `trace`), or that took `--min-ms` or longer (`getLogs`)    |
| `admin stats [--days <n>]`                                                                       | shows how the host is used over the last week or `--days` (up to 400), in UTC, with today last. It shows what the host holds (groups, those switched off, spaces, bytes of files) and a row for each day (requests, refused with a 4xx, failed with a 5xx, slow at a second or more, MCP tool calls, groups and spaces made). Then it lists each route asked, most asked first, and each tool called (`getStats`) |

An unknown command or flag fails with its usage, and any failure exits 1.

| Variable                               | What it is                                                                                                                                                                       |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ATGRP_AS`, `ATGRP_PASSWORD`           | the handle you sign in as and its password (an app password is enough)                                                                                                           |
| `PDS_URL`                              | where you sign in · `http://localhost:2583`                                                                                                                                      |
| `ATGRP_HOST`                           | the host · `http://localhost:2600`                                                                                                                                               |
| `ATGRP_HOST_DID`                       | its DID · the did:web of its address                                                                                                                                             |
| `ATGRP_SEED_INVITE`                    | who `group seed` invites, comma-separated                                                                                                                                        |
| `ATGRP_IMPORT_PASSWORD`                | the password at its own server of the account `account import` moves. It has to be the account's own password, since its server won't sign its identity over for an app password |

## `just`

| Recipe                     | What it does                                                                                                                                                                                                                                                                                         |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `setup`                    | installs the console's and the harness's packages, llvm-tools and cargo-llvm-cov                                                                                                                                                                                                                     |
| `test`, `t <args>`         | runs every test, or those `args` pick                                                                                                                                                                                                                                                                |
| `cov`, `cov-html`          | runs the tests under the 100% gate, or shows their coverage as a page                                                                                                                                                                                                                                |
| `fmt`                      | formats the Rust with rustfmt, and the console, the harness and the docs site with oxfmt                                                                                                                                                                                                             |
| `lint`                     | checks formatting, clippy, the console's layers and svelte-check                                                                                                                                                                                                                                     |
| `lex`                      | runs the lexicons' checks                                                                                                                                                                                                                                                                            |
| `dev`                      | runs the console over a world of its own to use: the reference PDS with alice, bob and harbor, the host, the console at https://group-host.localtest.me, and an invite waiting for alice ([development](../development.md#using-it))                                                                 |
| `serve`                    | runs the host                                                                                                                                                                                                                                                                                        |
| `web`, `web-dev`           | builds the console, or serves it as its source changes                                                                                                                                                                                                                                               |
| `look <path>… [options]`   | draws the console's pages as pictures in `target/look`, in any state: `--as <account>`, `--seed <name\|file>`, `--click`, `--type`, `--element`, `--phone`, `--desktop`, `--light`, `--dark`, `--all` ([development](../development.md#looking-at-the-console))                                      |
| `look-up`                  | runs a world for `look` to draw over, kept until ctrl-c                                                                                                                                                                                                                                              |
| `shots [--check] [guide]…` | redraws the guides' pictures and their text into `docs/guides/shots`, from the look line above each, with each guide over a fresh world. With `--check`, it writes nothing and fails if a picture's text has changed, as the `e2e` check does ([development](../development.md#the-guides-pictures)) |
| `e2e <args>`               | builds the console and the host, then runs the console's flows in Chromium over the reference PDS, as the `e2e` check does                                                                                                                                                                           |
| `atgrp <args>`             | runs the command                                                                                                                                                                                                                                                                                     |
| `docs`                     | checks the docs' links and anchors, as the `docs` check runs it                                                                                                                                                                                                                                      |
| `docs-site`                | serves the docs as their site at http://localhost:4321/group-host/, updating as they change                                                                                                                                                                                                          |
| `docs-deploy`              | builds the docs site and deploys it as the repository's ([development](../development.md#the-docs-site))                                                                                                                                                                                             |
| `release [--dry-run]`      | tags the release that last landed, then cuts the next from main's change files as the `Release <version>` pull ([development](../development.md#releasing))                                                                                                                                         |
