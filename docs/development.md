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

Its groups are named under `groups.localtest.me`, as
`riders.groups.localtest.me`, so a group's address opens in a browser:
a site published with `site publish` is at https://riders.groups.localtest.me,
sandboxed as it is when deployed, and a group with none sends `/` to its
page in the console. `just dev` builds the page around a site that names
`reads` (`web/shell`, by `npm run shell` in `web/`) before it starts, and
the host reads it from there; a change to it means starting again.

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
needs one, as do the console's flows that write in a space; CI runs both
on the space alpha, built by `harness/alpha.sh` at the commit it names,
and `GROUP_HOST_DEV_ENV` names such a build (an atproto checkout's
`packages/dev-env`) to run them here. Asked for so, `just e2e` stops at
once if the build named speaks no spaces, rather than skip those flows.

`just test` runs every test; `just t <cargo test args>` a selection;
`just cov` all of them under the gate the `test` check holds: 100% of
lines and functions, tests and the harness left out. A test that starts
the host or the command as a binary stops it with SIGTERM, so it exits
as it should and writes the coverage it counted.

## The console, end to end

`just e2e` uses the console as people use it, in Chromium, with
Playwright (`harness/e2e/flows.spec.mjs`): it signs in through the
reference PDS's own approval page, signs an app in as a group and has it
write as the group, publishes a site and checks that a page of it reaches
the console without the session of whoever opens it, signs in on a site
that shows its members to those who may see them, declines an app, and
signs out. Its setup (`harness/e2e/setup.mjs`) runs the reference
PDS, the host over it, and the built console behind an https front at
`console.group-host.internal` (`harness/tls-proxy.mjs`), as the PDS
reaches an OAuth client only over https at a hostname; `harness/pds.mjs`
makes a certificate for that name and every name beside it, and resolves
them. Hikers' and Riders' sites have fronts of their own at
`hikers.group-host.internal` and `riders.group-host.internal`, names
beside the console's, so the browser takes their pages for the same site
as the console, as it does a deployment's group addresses; each front
passes the name asked on as `X-Forwarded-Host`, which a site's OAuth
client is named for (the PDS refuses a client under `.test`). Chromium
comes with `cd harness && npx playwright install chromium`, and is told
that every name under `group-host.internal` is this machine. A failed
flow leaves its trace and a picture under `target/e2e/results`.

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

## The guides' pictures

A guide (`docs/guides/`) shows the console with pictures, and each is
drawn by `just look`, never taken by hand. The line above a picture says
how:

```md
<!-- look /g/lanterns.groups.test --as alice --seed full --click 'button[popovertarget=invite]' -->
![The invite dialog](shots/invite-dialog.png)
```

`just shots` draws them again (`harness/shots.mjs`), after a change to
the console a guide shows, or only those of the guides it is given. Each
guide is drawn over a fresh world of its own, its pictures in page
order, so what an earlier one did (a seed, a form sent) is there for the
ones after; a world `just look-up` keeps would bring its own, so none
may be up. Each picture is drawn at desktop size in both schemes,
`shots/<name>.png` and `shots/<name>-dark.png`, and the site shows the
one the reader's scheme asks for. Beside each is `shots/<name>.txt`,
what the picture shows as text: the page's accessibility tree, its
headings, buttons, labels and words, with what differs from one world
to the next (times, DIDs, keys, ports, a new password) as placeholders.

Pixels differ between any two runs, but the text does not unless the
console does, so the text is what is checked. `just shots --check`
draws every guide again and compares only the text, writing nothing,
and fails naming each picture whose page has changed; the `e2e` check
runs it. A guide whose console has moved on is drawn again with `just
shots`, and its words read against the new pictures. `just docs` fails
a picture with no look line, a name two pictures share, one with no
dark picture or no text, a picture in `shots/` no guide shows, and a
word a guide puts in bold, as on screen, that is in no guide's picture.
A picture's page is drawn from a list in no set order only where the
list does not show. The `guides` skill (`.claude/skills/guides`) is how
a guide is written.

## The docs site

`docs/` reads on its own, in the repository's pages; `docs-site/` makes
a site of it with Starlight, changing nothing in it (`content.mjs`
copies each page in, its first heading its title, and the guides'
pictures beside them). A link between pages
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
