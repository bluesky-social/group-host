# Working on group-host

For every agent working here: CLAUDE.md is this file, imported.

This is a group host for atproto, built in Rust and Svelte,
implementing the opensocial proposal
(https://tangled.org/opensocial.group/proposal). `docs/design/README.md`
says what is here, what is still to come, and in what order.

Use `rustup run stable cargo` (the Justfile's `cargo`), not a system cargo.
Node is the version `.node-version` names, CI's.

## Running tests

Build and test on the runner where you can: open the pull early, push to
its branch as you go, and read what its checks say with `knotless checks
knotless://chadtmiller.com/group-host --pull <uri> --wait <secs> --log`.
`just fmt` (rustfmt, and oxfmt for the console, the harness and the docs
site) costs nothing, so run it before pushing; Claude Code runs it on
each file it writes (`.claude/settings.json`).

Build here for a loop the runner is too slow for, such as one test being
fixed, with `CARGO_BUILD_JOBS=2`: other agents build on this machine too.

- `just t <cargo test args>` runs a selection; `just cov` the whole gate.
- A test that starts a binary (the host, the command) stops it with
  SIGTERM and waits, never kills it: a killed process writes no coverage,
  and its lines count as never run. The host stops gracefully for this,
  as it does for a deploy.
- `cargo fmt` can put a `?` on a line of its own, which coverage then
  counts as a line never run: bind the value first, or use a named
  function, rather than lowering the gate. A call taking a closure ends in
  `})?;`, the same line. The report may count such a line without naming
  it, as it counts each function at the build of it with the most lines;
  `--show-missing-lines` and the HTML then show nothing missed.

## Building a part

Each pull brings one part of the host, whole: its methods, its records,
its tests, and its page in `docs/`. The tests say what the part must get
right, and come before the code they test.

- The atproto primitives (DRISL, CIDs, keys, the MST, CAR, PLC, service
  auth) are the `atproto` crate, in its own repository
  (knotless.dev/chadtmiller.com/atproto), published to that repository's
  Cargo registry, which `.cargo/config.toml` names. A primitive the host
  needs goes there, as a pull of that repository and a new version, checked
  against atproto's interop test vectors and the reference implementation;
  then this workspace moves to that version.
- What a PDS, the OAuth provider and the space alpha do is written here
  as far as the host uses it, and no further.

## Layout

- `crates/group-host`: the server (axum, SQLite through rusqlite). It is
  the PDS for its groups, the authority for their spaces, their OAuth
  server, and answers the `fyi.opensocial.*` methods. It serves the
  console's pages' queries as XRPC, each with its lexicon under
  `lexicons/`.
- `crates/group-host-mcp`: the MCP protocol and tools, the core's
  methods asked as the member who connected an MCP client; the host serves
  them at `/mcp`.
- `crates/opensocial`: a client for group hosts, and the `opensocial`
  command (creating, inspecting and running groups; seeding; moving a group
  or a host), as `packages/cli` and the host's scripts were.
- `web/`: the console, a SvelteKit app. Its TypeScript types are made from
  the lexicons by @atproto/lex (`npm run lex`, which `dev`, `build` and
  `check` run first) into `web/src/lib/lexicons`, not kept in git.
- `packages/sdk`: `@opensocial/sdk`, for apps on group hosts: lex-space for
  a group's spaces and the host's own methods, in lex-space's style. The
  host's page around a site (`web/shell`) uses it. Its lexicon types are
  made by `npm run lex` into `packages/sdk/src/lexicons`, not kept in git.
- `docs-site/`: the docs as a site, made from `docs/` as it is, and
  deployed with `just docs-deploy` (`docs/development.md`).
- `harness/`: the checks beside the tests, and the console's flows end to
  end (`harness/e2e`).

## Commands and tools

A command is `opensocial <group> <action> <group> ...`: the group a
singular noun for what it acts on (`group`, `role`, `member`, `invite`,
`mod`), then the action, then the group it acts in, then the arguments
it takes in order, then flags. An opposite is `--undo` on the action
(`invite send --undo`), never an `un-` command. `opensocial --help`
lists them by group.

A command and its MCP tool land together, in one pull, from one entry in
`TOOLS` in `crates/group-host-mcp/src/lib.rs`. The tool is
`<group>_<action>`, and takes what the command takes but the group: its
arguments named as the usage names them, its flags in snake case
(`--display-name`, `display_name`; `--undo`, `undo`). The command reads
its words by the tool's schema, and both carry out the same `Plan`, so a
new one is an entry there, its line in the usage, and its row in
`docs/reference/cli.md` and `docs/reference/mcp.md`, each laid out by
group. One action is one tool: a tool that would fold several commands
is split. The commands that make, move or delete a group or an
account (`group create`, `group seed`, `group move`, `group delete`,
`account import`) have no tool, as an MCP client acts in the one group
it was connected to, and should never end it; nor have the `admin`
commands, which read the whole host for its operators. The
host's own command, `group-host move-host`, stands alone.

## The console

- Before changing anything under `web/`, load the `ui` skill
  (`.claude/skills/ui`): its house rules, and `just look`, which draws
  any page in any state as pictures to open (signed in as anyone, a
  group with nothing in it or everything, a dialog open, on a phone, in
  the dark). Look at every state you touch before pushing.
- The look is the console's own, all of it in `web/src/app.css` over the
  tokens: warm paper, sections as cards (`Section.svelte`), one colour
  that acts (`--primary`), and rounded headings. A page takes the look
  from the shared rules, never from styles of its own: a new kind of
  thing on a page gets a rule there, written for every page that shows
  it. A button that undoes, removes or turns down is `class="quiet"`.
- The components in `web/src/lib` are layered, atomic design's way:
  `atoms` (one element: an avatar), `molecules` (a few atoms that mean
  something together: a group's row), `organisms` (a part of a page: a
  list of groups), `templates` (a page's layout, without its data). A
  component uses its own layer and those below it, never one above;
  `node harness/layers.mjs` checks it. Pages under `web/src/routes` fill a
  template with what the queries answer, through `#lib/data.ts`.
- The styles are `web/src/app.css`, and every colour, size, space, radius
  and weight in it is a token from `web/src/tokens.css`: take one by name
  rather than writing a value, and add one there when none fits. A colour
  for what it means is its role (`--danger`, `--success`, `--attention`,
  `--done`, `--neutral`), never the hue under it.
- In Svelte markup, keep a run of inline text one expression or one line.
- An act on a page is sent in place (`use:enhance` with `#lib/forms.ts`),
  never a plain post that reloads it at its top; the ui skill says how.
- A dialog's title is fixed words ("Remove this member?"), never a name,
  handle or anything typed, which can be any length; the body says which
  thing. `node harness/dialogs.mjs` checks it.
- Sentence case everywhere; an empty list says what is empty in the
  reader's terms, and the way out (`Empty.svelte`).

## Lexicons

The schemas under `lexicons/` are this host's to change until they are
published from here. Until then, `just lex` lets a schema, def or field
go when nothing here refers to it; `PUBLISHED` in `harness/lexicons.mjs`
turns that off once the first is published.
Once one is published, it is evolved, never changed, with new fields
optional and nothing removed, renamed, retyped, made required or narrowed.
A method's lexicon lists in `errors` each error it answers by name beyond
those any method may (`docs/reference/api.md` lists them), each with a
description of when: a client is to tell one refusal from another by its
name, never by its message. An error the host cannot reach from that
method is not listed.

Every schema of ours (`fyi.opensocial.*`) follows the Lexicon Style Guide
(https://atproto.com/guides/lexinomicon) and passes `just lex`: goat's
linter, then `harness/lexicons.mjs` for what it does not see (`knownValues`
rather than `enum`; arrays of objects, which can grow, not bare values,
with the small shared ones in `fyi.opensocial.defs`; a description on
every `main`; a `maxLength` on strings people write; accounts by DID, a
handle only as an account's own `handle`), and that every schema only
grows from main's. Nothing is let off but what the space alpha brings and
goat does not know yet: the `space-ref` format and permissions over a
`space`. The schemas under `lexicons/upstream` and `community/` are
others', copied as they are for what ours refer to.

## Docs

`docs/` is a small wiki: design pages explain why, the guides show how,
the reference lists what. A change that alters what a page says updates
that page in the same pull:

- what is built and what is next: `docs/design/README.md`
- an environment variable: `docs/reference/config.md`
- an XRPC method or route: `docs/reference/api.md`; an MCP tool:
  `docs/reference/mcp.md`
- a new page: the sidebar in `docs-site/astro.config.mjs`
- a command of `opensocial`, or a `just` recipe: `docs/reference/cli.md`
- building, testing, the checks: `docs/development.md`
- a flow in the console a guide shows: that guide, its pictures drawn
  again with `just shots` (the `e2e` check fails until they are)

A design page arrives with its part, written from the code here, and
says what this host does, never what another does or where an idea came
from. The `docs` skill (`.claude/skills/docs`) is how a page is written
or checked: facts from the code first, then the page fresh. A guide is
written by the `guides` skill (`.claude/skills/guides`) the same way,
its facts from walking the console with `just look`, its pictures drawn
by `just shots` from the line above each.

## What CI checks

The workflows in `.knotless/workflows/` run on pushes to `main` and on
pulls into it or into a `feature/` branch:

- `check`: `cargo fmt --check` and clippy, warnings denied.
- `test`: every test, under the 100% gate of lines and functions.
- `web`: oxfmt's format, the component layers, dialogs' titles,
  svelte-check (failing on any warning), and the build.
- `lexicons`: `just lex`.
- `e2e`: `just e2e`, the console in Chromium over the reference PDS,
  then `just shots --check`: each guide's pictures drawn again, failing
  when what one shows has changed.
- `docs`: `just docs`, every link and anchor landing, and the docs site's
  build.

Two more check nothing. `mirror`: each push to `main` or a tag pushes
the same commit to a private copy on GitHub
(github.com/bluesky-social/group-host), for those who read the code
there. Nothing is pushed to that copy but from here. `deploy`: each push
to `main`, which takes a pull only once its checks pass,
is built on the machine serving next.opensocial.fyi and goes up there
(`docs/deploying.md`). Every workflow but `deploy` names
`runner.knotless.dev`; `deploy` alone runs on `ci.knotless.dev`, which
runs only its owner's repositories, and holds its secrets.

On a pull, `web`, `lexicons`, `e2e` and `docs` run only when it changes
what they read; otherwise each passes at once, noted as skipped.

## Landing a change

Open a pull rather than committing to `main`: push the work to a branch
named for it (`atproto/mst`, `host/groups`), then

```sh
knotless pull knotless://chadtmiller.com/group-host main \
  knotless://chadtmiller.com/group-host <branch> "<title>" -m - < body.md
```

the title and body written from `.knotless/pull.md`; `-m -` keeps the shell
off the Markdown. Commit titles stay prose, no conventional commits. Once
its checks pass, `knotless merge knotless://chadtmiller.com/group-host
<pull uri> --wait <secs>` squashes it onto `main`. A pull that conflicts
with `main` fails each check without running: merge `main` into it.

Work that should not reach `main` yet goes on a `feature/<name>` branch,
its parts pulls into it.
