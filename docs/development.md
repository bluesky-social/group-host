# Development

You need Rust's stable toolchain through rustup, Node at the version
`.node-version` names, `just`, and goat (github.com/bluesky-social/goat)
for the lexicons.

```sh
just setup     # the console's and harness's packages, and coverage tools
just dev       # all of it, to use: see below
just serve     # the host alone, on http://localhost:2600
just web-dev   # the console alone, asking that host
```

## Using it

`just dev` starts a world of its own (`harness/dev.mjs`): the reference
PDS and a PLC directory with alice, bob and harbor on them, the host
over them, and the console at https://group-host.localtest.me, with
Riders, a group of alice's, and Hikers, a group of bob's that has
invited alice, so her invite waits when she signs in. The console is the dev server's, so a change to a page
shows at once; a change to the host means starting it again. Each
account signs in as its handle (`alice.test`, `bob.test`,
`harbor.test`) with the password `password`. It prints them, where the
codes the PDS emails are read, and how to run the command against it. ctrl-c stops it all, and each start is empty
again.

Signing in to the console takes https at a name: the host is the
console's OAuth client, and a PDS fetches a client's metadata only so.
portless (a harness package) serves it: its proxy on port 443 sends
every name under localtest.me (which public DNS resolves to 127.0.0.1)
to the port registered for it, with a certificate from a local CA in the
system's trust store, which the PDS is told to trust too. Start its
proxy once a boot, as `just dev` says if it is not up; it asks for your
password to take port 443.

## Tests

The tests that reach the network run against the reference PDS and a PLC
directory, which `harness/pds.mjs` starts once for each test binary that
asks (`tests/common/net.rs`), on Node at the version `.node-version`
names. On npm's PDS, which has no spaces, the tests of a space skip what
needs one; CI runs them on the space alpha, built at the commit
`.knotless/workflows/test.yml` names, and `GROUP_HOST_DEV_ENV` names
such a build (an atproto checkout's `packages/dev-env`) to run them
here.

`just test` runs every test; `just t <cargo test args>` a selection;
`just cov` all of them under the gate the `test` check holds: 100% of
lines and functions, tests and the harness left out. A test that starts
the host or the command as a binary stops it with SIGTERM, so it exits
as it should and writes the coverage it counted.

## The console, end to end

`just e2e` uses the console as people use it, in Chromium, with
Playwright (`harness/e2e/flows.spec.mjs`): it signs in through the
reference PDS's own approval page, signs an app in as a group and has it
write as the group, declines one, and signs out. Its setup
(`harness/e2e/setup.mjs`) runs the reference PDS, the host over it, and
the built console behind an https front at `console.group-host.internal`
(`harness/tls-proxy.mjs`), as the PDS reaches an OAuth client only over
https at a hostname; `harness/pds.mjs` makes that name's certificate and
resolves it. Chromium comes with `cd harness && npx playwright install
chromium`. A failed flow leaves its trace and a picture under
`target/e2e/results`.

## Looking at the console

`just look` draws the console's pages as pictures, in any state, for an
agent (or you) to open: it is how a page is seen to look right, where
the flows only see it work. The `ui` skill (`.claude/skills/ui`) says
which states to look at and what the pages are held to.

```sh
just look /g/riders.groups.test --as alice           # signed in as alice
just look / --as bob,alice --click '.user-menu summary'  # signed in to both, alice in use
just look / --seed full --as bob --phone --dark     # a seeded state, on a phone, dark
just look /g/lanterns.groups.test --as alice --click 'button[popovertarget=invite]'
```

It draws over a world as the flows' is (`harness/world.mjs`), behind the
same https front, but with the console served by its dev server, so the
pictures show the source as it is now, with no build. Signing in as an
account goes through its PDS's own page, as a person does, and is kept
for the next look. A seed (`--seed`, from `harness/look/seeds` or a
file of your own) writes the state a page should show: a group in full,
everything long, people waiting, an app that wrote as a group. Starting a world takes half a minute;
`just look-up`, left running, keeps one for every look until ctrl-c,
each seed written to it once. The pictures go to `target/look`
(`--out` for another place), desktop light and phone dark unless asked
otherwise, and it says when a page is wider than the screen, naming the
element that runs past its edge. Its options are at the top of
`harness/look/look.mjs`.

## The docs site

`docs/` reads on its own, in the repository's pages; `docs-site/` makes
a site of it with Starlight, changing nothing in it (`content.mjs`
copies each page in, its first heading its title). A link between pages
goes to the page on the site, and one out of `docs/` to the file in the
repository (`plugins.mjs`). On a reference page, a table of routes or
methods is drawn as a list, each with its HTTP method, and each
`fyi.opensocial` method, or MCP tool, with its lexicon from `lexicons/`
under it, so what it takes and answers cannot drift from the page
(`reference.mjs`). A method named in code on any page links to its entry
there, by its whole NSID, or by its name alone where no other method
listed has that name. Its look is the console's, from `web/src/tokens.css`.

`just docs-site` serves it as it changes, at http://localhost:4321/. A
new page goes in the sidebar in `docs-site/astro.config.mjs`.

The repository is private, so its site is too: knotless serves it to its
members alone, at a host of its own, which `just docs-deploy` prints. It
is kept in the repository's space, which only its owner can write,
signed in with `knotless login`; an app password cannot write to a
space, so no workflow can deploy it, and the owner runs `just
docs-deploy` after a change to the docs lands.

## The checks

`just lint` runs what `check` and `web` do but the console's build:
`cargo fmt --check`, `oxfmt --check`, clippy with warnings denied, the
console's component layers (`harness/layers.mjs`), that no dialog's
title holds a name or anything typed (`harness/dialogs.mjs`), and
svelte-check.

`just fmt` formats everything the checks hold to a format: the Rust with
rustfmt, and the console, the harness and the docs site with
[oxfmt](https://oxc.rs/docs/guide/usage/formatter), set in
`.oxfmtrc.json` (two spaces, single quotes, 100 columns, `.svelte` files
included). An agent need not run it: `.claude/settings.json` has Claude
Code format each file as it is written (`.claude/hooks/format.mjs`).

`just lex` is the `lexicons` check, with goat 0.2.8 in CI, and `just
docs` with the site's build the `docs` check. `AGENTS.md` says what each workflow runs,
and when.
