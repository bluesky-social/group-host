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

`just dev` starts a world of its own (`harness/dev.mjs`). It runs the
reference PDS and a PLC directory with alice, bob and harbor on them,
the host over them, and the console at https://group-host.localtest.me.
There are two groups. Riders is alice's, and Hikers is bob's and has
invited alice, so her invite is waiting when she signs in.

The console runs on its dev server, so a change to a page shows at once.
A change to the host means starting it again. Each account signs in as
its handle (`alice.test`, `bob.test`, `harbor.test`) with the password
`password`. `just dev` prints these, where to read the codes the PDS
emails, and how to run the command against it. ctrl-c stops it all, and
each start is empty again.

Its groups are named under `groups.localtest.me`, as
`riders.groups.localtest.me`, so a group's address opens in a browser,
and sends `/` to the group's page in the console, as it does when
deployed.

Signing in to the console takes https at a name. That's because the
host is the console's OAuth client, and a PDS only fetches a client's
metadata that way. portless (a harness package) serves it. Its proxy on
port 443 sends every name under localtest.me (which public DNS resolves
to 127.0.0.1) to the port registered for it. It uses a certificate from
a local CA in the system's trust store, and the PDS is told to trust
that CA too. Start its proxy once a boot, as `just dev` tells you if
it isn't up. It asks for your password to take port 443.

## Tests

The tests that reach the network run against the reference PDS and a PLC
directory. `harness/pds.mjs` starts them once for each test binary that
asks (`tests/common/net.rs`), on Node at the version `.node-version`
names. npm's PDS has no spaces, so on it the tests of a space skip what
needs one, and so do the console's flows that write in a space. CI runs
both on the space alpha, built by `harness/alpha.sh` at the commit it
names. To run them here, point `GROUP_HOST_DEV_ENV` at such a build (an
atproto checkout's `packages/dev-env`). When it's set, `just e2e` stops
at once if that build doesn't speak spaces, instead of skipping those
flows.

`just test` runs every test, and `just t <cargo test args>` runs a
selection. `just cov` runs all of them under the gate the `test` check
holds, which is 100% of lines and functions, with tests and the harness
left out. A test that starts the host or the command as a binary stops
it with SIGTERM, so it exits cleanly and writes the coverage it counted.

## The console, end to end

`just e2e` uses the console the way people do, in Chromium, with
Playwright (`harness/e2e/flows.spec.mjs`). It signs in through the
reference PDS's own approval page, signs an app in as a group and has it
write as the group. It also starts and runs groups, invites, moderates,
declines an app, and signs out.

Its setup (`harness/e2e/setup.mjs`) runs the reference PDS, the host
over it, and the built console behind an https front at
`console.group-host.internal` (`harness/tls-proxy.mjs`). That's because
the PDS only reaches an OAuth client over https at a hostname.
`harness/pds.mjs` makes a certificate for that name and every name
beside it, and resolves them. Hikers' and Riders' addresses have fronts
of their own at `hikers.group-host.internal` and
`riders.group-host.internal`, beside the console's, as a deployment's
group addresses are. Install Chromium with
`cd harness && npx playwright install chromium`. It's told that every
name under `group-host.internal` is this machine. A failed flow leaves
its trace and a picture under `target/e2e/results`.

After going to a console page, each flow waits until the page has
hydrated (the root layout sets `data-hydrated` on `<html>`). That's
because hydration puts back a field filled before it.

## Looking at the console

`just look` draws the console's pages as pictures, in any state, for an
agent (or you) to open. The flows only see a page work, and this is how
you see that it looks right. The `ui` skill (`.claude/skills/ui`) says
which states to look at and what the pages are held to.

```sh
just look /g/riders.groups.test --as alice           # signed in as alice
just look / --as bob,alice --click '.user-menu summary'  # signed in to both, alice in use
just look / --seed full --as bob --phone --dark     # a seeded state, on a phone, dark
just look /g/lanterns.groups.test --as alice --click 'button[popovertarget=invite]'
```

It draws over a world like the flows' one (`harness/world.mjs`), behind
the same https front. But the console runs on its dev server, so the
pictures show the current source with no build. Signing in as an
account goes through its PDS's own page, the way a person does, and the
session is kept for the next look. A seed (`--seed`, from
`harness/look/seeds` or a file of your own) writes the state a page
should show. Examples are a group in full, everything long, people
waiting, or an app that wrote as a group.

Starting a world takes half a minute. `just look-up`, left running,
keeps one for every look until ctrl-c, and writes each seed to it once.
The pictures go to `target/look` (`--out` for another place), desktop
light and phone dark unless you ask otherwise. It tells you when a page
is wider than the screen, naming the element that runs past its edge.
Its options are at the top of `harness/look/look.mjs`.

## The console's languages

Every word the console shows is a message in `web/messages/en.json`,
compiled by [Paraglide](https://paraglidejs.com) into functions under
`web/src/lib/paraglide` (made as the console builds, and by `npm run
i18n`, not kept in git). A page calls one as `m.invites_empty_title()`.
`node harness/strings.mjs` fails on words written straight into a
component. English is the only language so far.

Each request's language is the one the `PARAGLIDE_LOCALE` cookie names,
else the first of the browser's `Accept-Language` the console has, else
English. The page's `<html lang>` and `dir` say it. Dates and lists
follow it (`#lib/locale.ts`), and a label shows its name in it when the
group gave one. Times stay in UTC, since the page is drawn on the
server. The host's own error messages
stay English, and the console says each by its name (`#lib/errors.ts`).
A few names (`InvalidRequest`, `InvalidHandle`,
`InvalidArchive`, `UpstreamFailure`) cover several reasons the reader
needs to tell apart, so for those the console shows the host's message.
A language other than English needs the host to give each such reason
a name of its own first.

Adding a language takes three changes. Add it to `locales` in
`web/project.inlang/settings.json`, add `web/messages/<language>.json`
with every key `en.json` has, and add its `Intl` locale to `INTL` in
`web/src/lib/locale.ts`. `web/src/app.css` lays pages out by left and
right, so a right-to-left language needs that changed first.

## The guides' pictures

A guide (`docs/guides/`) shows the console with pictures. `just look`
draws each one, and none is taken by hand. The line above a picture says
how:

```md
<!-- look /g/lanterns.groups.test --as alice --seed full --click 'button[popovertarget=invite]' -->
![The invite dialog](shots/invite-dialog.png)
```

`just shots` draws them again (`harness/shots.mjs`) after a change to
the console a guide shows. Give it guides and it draws only theirs. It
draws each guide over a fresh world of its own, with its pictures in
page order, so what an earlier one did (a seed, a form sent) is there
for the ones after. A world kept by `just look-up` would bring its own
state, so none may be up. Each picture is drawn at desktop size in both
schemes, `shots/<name>.png` and `shots/<name>-dark.png`, and the site
shows the one the reader's scheme asks for. Beside each is
`shots/<name>.txt`, which is what the picture shows as text. That's the
page's accessibility tree (its headings, buttons, labels and words),
with placeholders for what differs from one world to the next (times,
DIDs, keys, ports, a new password).

Pixels differ between any two runs, but the text doesn't unless the
console does, so the text is what's checked. `just shots --check` draws
every guide again and compares only the text, writing nothing. It fails
naming each picture whose page has changed, and the `e2e` check runs
it. When a guide's console has moved on, draw it again with `just
shots` and read its words against the new pictures.

`just docs` fails a picture with no look line, a name two pictures
share, and a picture with no dark picture or no text. It also fails a
picture in `shots/` that no guide shows, and a word a guide puts in
bold, as on screen, that's in no guide's picture.

A list in no set order changes from one run to the next, so a picture
shows one only cropped to where its order doesn't show. The `guides`
skill (`.claude/skills/guides`) says how to write a guide.

## The docs site

`docs/` reads on its own, in the repository's pages. `docs-site/` makes
a site of it with Starlight and changes nothing in it. `content.mjs`
copies each page in, with its first heading as its title, and the
guides' pictures beside them. A link between pages goes to the page on
the site, and a link out of `docs/` goes to the file in the repository
(`plugins.mjs`).

On a reference page, a table of routes or methods is drawn as a list,
each with its HTTP method. Each `org.atgrp` method, or MCP tool,
gets its lexicon from `lexicons/` under it, so what it takes and answers
can't drift from the page (`reference.mjs`). A method named in code on
any page links to its entry there. It matches by its whole NSID, or by
its name alone where no other listed method has that name. The site
looks like the console, from `web/src/tokens.css`.

`just docs-site` serves it as it changes, at http://localhost:4321/group-host/. A
new page goes in the sidebar in `docs-site/astro.config.mjs`.

knotless serves it as the repository's site, at
https://chadtmiller-com.knotless.page/group-host/, so it's built with
`/group-host/` as its base. The `docs` check deploys it on each push to
`main`, as the owner, with the app password `RELEASE_PASSWORD` that
`release` takes too. `just docs-deploy` deploys a build from here, as
the owner signed in with `knotless login`.

## The checks

`just lint` runs what `check` and `web` do, except the console's build.
That's `cargo fmt --check`, `oxfmt --check`, clippy with warnings
denied, `cargo deny check`, the console's component layers
(`harness/layers.mjs`), a check that no dialog's title holds a name or
anything typed (`harness/dialogs.mjs`), a check that no component writes
words into its markup rather than saying a message
(`harness/strings.mjs`), and svelte-check.

`cargo deny check` holds the dependencies to `deny.toml`: no advisory of
the RustSec database, unmaintained and unsound crates and yanked
versions too; a permissive licence for every one; and crates from
crates.io and the atproto registry alone. An advisory is fixed by an
upgrade, never ignored. The advisories are read as they are at the run,
so `check` can fail on `main` the day one is published, with nothing
changed. It needs `cargo install --locked cargo-deny` once; CI runs
0.22.0.

No crate has unsafe code: `[workspace.lints.rust]` in `Cargo.toml`
forbids it, and each crate takes the workspace's lints, so the build
refuses unsafe code in every target, tests too. A new crate takes them
with `[lints] workspace = true`.

`just fmt` formats everything the checks hold to a format. It formats
the Rust with rustfmt, and the console, the harness and the docs site
with [oxfmt](https://oxc.rs/docs/guide/usage/formatter), set in
`.oxfmtrc.json` (two spaces, single quotes, 100 columns, `.svelte` files
included). An agent doesn't need to run it, since
`.claude/settings.json` has Claude Code format each file as it's
written (`.claude/hooks/format.mjs`).

`just lex` is the `lexicons` check, with goat 0.2.8 in CI. `just docs`
with the site's build is the `docs` check. `AGENTS.md` says what each
workflow runs, and when.

## Releasing

The workspace has one version, in `Cargo.toml`, which `/xrpc/_health`
and the MCP server's `serverInfo` name. It's set with
[knope](https://knope.tech) from change files, not conventional commits.

A pull that changes what someone running a group, calling the host, or
using the command or the console sees adds a change file under
`.changeset/`, as `knope document-change` writes one. Its front matter
names the bump (`default: patch`, `minor` or `major`), and below it a
heading and a line say what changed for them. The `check` check asks
for one on any pull that changes `crates/*/src`, `web/src`,
`web/messages` or `lexicons/org` (`harness/changes.sh`). A change no one
sees says `Changeset: none` on a line of one of its commits' messages
instead.

`just release` (`deploy/release.sh`) cuts a release from main's change
files. It works out the version: a `minor` or `major` file bumps 0.x's
minor, where knope alone would bump the patch, and a `patch` file the
patch; from 1.0 on, each bumps its own part. It runs `knope release
--override-version <version>` at main's tip, in a worktree of its own,
which bumps `Cargo.toml` and the crates' entries in `Cargo.lock`, writes
the version's section of `CHANGELOG.md`, and deletes the change files.
It commits that as `Release <version>`, force-pushes it to
`release/next`, and opens the pull of that title with the changelog in
its body, or brings the open one up to date. A pull that already holds
main's change files and still merges with main is left alone, so its
checks don't run again for nothing.

Merging that pull is the release: `release` publishes its image
([below](#the-image)). The next `just release` tags its commit
`v<version>` before it cuts anything, and `mirror` pushes the tag to
GitHub. `just release --dry-run` tags nothing, makes the release commit,
and prints the pull's body, pushing nothing.

The owner runs it, signed in with `knotless login`, with knope
installed (`cargo install knope`).

## The image

Each version's image is published by `.knotless/workflows/release.yml`, as the release of the `group-host` package, which operators [pull](deploying.md#pulling-the-image) from knotless.dev. It runs on a push to `main` that changes `Cargo.toml`, once that commit's checks have passed, and publishes when `main`'s version has a section in `CHANGELOG.md` and no release yet, so it publishes once for each Release pull that lands, with that section as the release's notes.

It builds what the `Dockerfile` builds: the console (`npm run build` in `web/`), then `group-host` and `atgrp` in the `deploy` profile, with the commit compiled into the host. The runner gives its jobs no container engine, so `.knotless/image.sh` makes the image the `Dockerfile`'s last stage would, as an OCI image layout, with no engine: `node:24.22.1-bookworm-slim` by digest, and one layer more holding the binaries, the console, a static tini and the CA bundle, with the same user, environment, working directory, entry point and command. A change to that stage changes `image.sh` with it. Its labels name the version and the commit. `knotless release publish --oci` publishes it.

It publishes as the owner, with `RELEASE_PASSWORD`, an app password of theirs, sealed to the shared runner for `main` alone. Without it, the publish step fails. Asked for on another branch, it builds the image and publishes nothing:

```sh
knotless secret set knotless://chadtmiller.com/group-host RELEASE_PASSWORD \
  --runner runner.knotless.dev --ref refs/heads/main
knotless run rerun knotless://chadtmiller.com/group-host --ref refs/heads/<branch> --workflow release
```

The image it published is then copied, digest and all, to GitHub's registry beside the repository's copy there, `ghcr.io/bluesky-social/group-host:<version>`, tagged `latest` too while it is the highest version. That takes `GHCR_TOKEN`, a GitHub token of `GHCR_USER`'s (the workflow's `env`) with `write:packages`, sealed the same way. Without it, nothing is copied. A version published before the copy existed, or whose copy failed, is copied when the workflow is asked for on `main` again; it builds nothing then, as the version is published already:

```sh
knotless secret set knotless://chadtmiller.com/group-host GHCR_TOKEN \
  --runner runner.knotless.dev --ref refs/heads/main
knotless run rerun knotless://chadtmiller.com/group-host --ref refs/heads/main --workflow release
```
