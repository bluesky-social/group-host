# Working on group-host

For every agent working here: CLAUDE.md is this file, imported.

This is a group host for atproto, built in Rust and Svelte as knotless is,
implementing the opensocial proposal
(https://tangled.org/opensocial.group/proposal). `docs/design/README.md`
says what is here, what is still to come, and in what order.

Use `rustup run stable cargo` (the Justfile's `cargo`), not a system cargo.
Node is the version `.node-version` names, CI's.

## Running tests

Build and test on the runner where you can: open the pull early, push to
its branch as you go, and read what its checks say with `knotless checks
knotless://chadtmiller.com/group-host --pull <uri> --wait <secs> --log`.
`cargo fmt` costs nothing, so run it before pushing.

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
  auth) are this repo's own, in `crates/atproto`, as knotless writes its
  own: few dependencies, every line under the gate. Check each against
  atproto's interop test vectors (github.com/bluesky-social/atproto-interop-tests)
  and the reference implementation, not only against itself.
- What a PDS, the OAuth provider and the space alpha do is written here
  as far as the host uses it, and no further.

## Layout

- `crates/atproto`: the primitives above.
- `crates/group-host`: the server (axum, SQLite through rusqlite). It is
  the PDS for its groups, the authority for their spaces, their OAuth
  server, and answers the `fyi.opensocial.*` methods. It serves the
  console's pages' queries as XRPC, each with its lexicon under
  `lexicons/`.
- `crates/group-host-mcp`: the MCP protocol and tools, the core's
  methods asked as the member who connected an assistant; the host serves
  them at `/mcp`.
- `crates/opensocial`: a client for group hosts, and the `opensocial`
  command (creating, inspecting and running groups; seeding; moving a group
  or a host), as `packages/cli` and the host's scripts were.
- `web/`: the console, a SvelteKit app. Its TypeScript types are made from
  the lexicons by @atproto/lex (`npm run lex`, which `dev`, `build` and
  `check` run first) into `web/src/lib/lexicons`, not kept in git.
- `docs-site/`: the docs as a site, made from `docs/` as it is, and
  deployed with `just docs-deploy` (`docs/development.md`).
- `harness/`: the checks beside the tests, and the console's flows end to
  end (`harness/e2e`).

## The console

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
- Sentence case everywhere; an empty list says what is empty in the
  reader's terms, and the way out (`Empty.svelte`).

## Lexicons

The schemas under `lexicons/` are this host's to change until they are
published from here.
Once one is published, it is evolved, never changed, with new fields
optional and nothing removed, renamed, retyped, made required or narrowed.
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

A design page arrives with its part, written from the code here, and
says what this host does, never what another does or where an idea came
from. The `docs` skill (`.claude/skills/docs`) is how a page is written
or checked: facts from the code first, then the page fresh.

## What CI checks

The workflows in `.knotless/workflows/` run on pushes to `main` and on
pulls into it or into a `feature/` branch:

- `check`: `cargo fmt --check` and clippy, warnings denied.
- `test`: every test, under the 100% gate of lines and functions.
- `web`: the component layers, svelte-check (failing on any warning), and
  the build.
- `lexicons`: `just lex`.
- `e2e`: `just e2e`, the console in Chromium over the reference PDS.
- `docs`: `just docs`, every link and anchor landing, and the docs site's
  build.

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
