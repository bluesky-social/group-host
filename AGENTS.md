# Working on group-host

For every agent working here: CLAUDE.md is this file, imported.

This is a group host for atproto, built in Rust and Svelte as knotless is.
Its first take is the prototype at `~/code/opensocial`: the core of its host
(`packages/group-host`) and the console's pages (`apps/web`). That prototype
is what we learned from, not a spec: this host does what its core does, in
the shape that is right here. `docs/design/README.md` says what is here,
what is still to come, and in what order.

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
  function, rather than lowering the gate.

## Building from the prototype

Each pull brings one part of the host, whole: its methods, its records,
its tests, and its page in `docs/`. Read how the prototype does that part,
and its tests, for what it has to handle; then write it as it should be.

- Not one to one. A method, record or field the prototype has can be
  renamed, merged, reshaped or left out when that makes the host simpler
  or clearer; the pull says what changed from the prototype and why. Only
  the prototype's core comes over, never its sites module (sites,
  templates, drafts, the network index, notifications).
- Its tests say what a part must get right; the cases they hold become
  tests here, in this host's terms, before the code they test.
- The atproto primitives (DRISL, CIDs, keys, the MST, CAR, PLC, service
  auth) are this repo's own, in `crates/atproto`, as knotless writes its
  own: few dependencies, every line under the gate. Check each against
  atproto's interop test vectors (github.com/bluesky-social/atproto-interop-tests)
  and the reference implementation, not only against itself.
- What the prototype gets from `@atproto/*` packages (a PDS's repo
  handling, the OAuth provider, the space alpha) is written here as far as
  the host uses it, and no further.

## Layout

- `crates/atproto`: the primitives above.
- `crates/group-host`: the server (axum, SQLite through rusqlite). It is
  the PDS for its groups, the authority for their spaces, their OAuth
  server, and answers the `fyi.opensocial.*` methods. It serves the
  console's pages' queries as XRPC, each with its lexicon under
  `lexicons/`.
- `crates/group-host-mcp`: an MCP server whose tools are the core's
  methods, asked as the member who connected it.
- `crates/opensocial`: a client for group hosts, and the `opensocial`
  command (creating, inspecting and running groups; seeding; moving a group
  or a host), as `packages/cli` and the host's scripts were.
- `web/`: the console, a SvelteKit app. Its TypeScript types are made from
  the lexicons by @atproto/lex (`npm run lex`, which `dev`, `build` and
  `check` run first) into `web/src/lib/lexicons`, not kept in git.
- `harness/`: the checks beside the tests.

## The console

- The pages are deliberately plain for now: the components work and are
  laid out, and the look comes later. Keep them so, rather than styling
  one page ahead of the rest.
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

The schemas under `lexicons/` are the prototype's core, as a starting
point: they are this host's to change until they are published from here.
Once one is published, it is evolved, never changed, with new fields
optional and nothing removed, renamed, retyped, made required or narrowed.
A new or changed schema follows the Lexicon Style Guide
(https://atproto.com/guides/lexinomicon) and passes `just lex`: goat's
linter, then `harness/lexicons.mjs` for what it does not see (`knownValues`
rather than `enum`; objects in new arrays; a description on every `main`;
a `maxLength` on strings people write; accounts by DID), and that every
schema only grows from main's. The findings the prototype's schemas came
with are listed in `harness/lexicons-kept.txt`, one a line, so the check
passes on them meanwhile; a new finding fails. They are debt: the pull that
builds a part fixes its schemas' findings and takes their lines out. The
space alpha's `space` type and `space-ref` format stay, as goat does not
know them yet.

## Docs

`docs/` is a small wiki: design pages explain why, the guides show how,
the reference lists what. A change that alters what a page says updates
that page in the same pull:

- what is built and what is next: `docs/design/README.md`
- an environment variable: `docs/reference/config.md`
- a command of `opensocial`, or a `just` recipe: `docs/reference/cli.md`
- building, testing, the checks: `docs/development.md`

Design pages for each part (groups, membership, signing in, moderation)
arrive with the part, written from the code here; the prototype's
`docs/design/` is background, not a source to copy.

## What CI checks

The workflows in `.knotless/workflows/` run on pushes to `main` and on
pulls into it or into a `feature/` branch:

- `check`: `cargo fmt --check` and clippy, warnings denied.
- `test`: every test, under the 100% gate of lines and functions.
- `web`: the component layers, svelte-check (failing on any warning), and
  the build.
- `lexicons`: `just lex`.

On a pull, `web` and `lexicons` run only when it changes what they read;
otherwise each passes at once, noted as skipped.

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
