# Development

You need Rust's stable toolchain through rustup, Node at the version
`.node-version` names, `just`, and goat (github.com/bluesky-social/goat)
for the lexicons.

```sh
just setup     # the console's packages, and cargo-llvm-cov
just dev       # all of it, to use: see below
just serve     # the host alone, on http://localhost:2600
just web-dev   # the console alone, asking that host
```

## Using it

`just dev` starts a world of its own (`harness/dev.mjs`): the reference
PDS and a PLC directory with alice, bob and harbor on them, the host over
them, and the console at https://group-host.localtest.me, with a group of
alice's. It prints each one's password, where the codes the PDS emails
are read, and how to run the command against it. ctrl-c stops it all, and
each start is empty again.

Signing in to the console takes https at a name: the host is the
console's OAuth client, and a PDS fetches a client's metadata only so.
portless (a harness package) serves it: its proxy on port 443 sends every
name under localtest.me (which public DNS resolves to 127.0.0.1) to the
port registered for it, with a certificate from a local CA in the system's
trust store, which the PDS is told to trust too. Start its proxy once a
boot, as `just dev` says if it is not up; it asks for your password to
take port 443.

## Tests

The tests that reach the network run against the reference PDS and a PLC
directory, which `harness/pds.mjs` starts once for each test binary that
asks (`tests/common/net.rs`), on Node at the version `.node-version` names.
On npm's PDS, which has no spaces, the tests of a space skip what needs
one; CI runs them on the space alpha, built at the commit
`.knotless/workflows/test.yml` names, and `GROUP_HOST_DEV_ENV` names such a
build (an atproto checkout's `packages/dev-env`) to run them here.

`just test` runs every test; `just t <cargo test args>` a selection;
`just cov` all of them under the gate the `test` check holds: 100% of
lines and functions, tests and the harness left out. A test that starts
the host or the command as a binary stops it with SIGTERM, so it exits as
it should and writes the coverage it counted.

## The console, end to end

`just e2e` uses the console as people use it, in Chromium, with Playwright
(`harness/e2e/flows.spec.mjs`), as knotless's harness does its pages: it
signs in through the reference PDS's own approval page, signs an app in as
a group and has it write as the group, declines one, and signs out. Its
setup (`harness/e2e/setup.mjs`) runs the reference PDS, the host over it,
and the built console behind an https front at `console.group-host.internal`
(`harness/tls-proxy.mjs`), as the PDS reaches an OAuth client only over
https at a hostname; `harness/pds.mjs` makes that name's certificate and
resolves it. Chromium comes with `cd harness && npx playwright install
chromium`. A failed flow leaves its trace and a picture under
`target/e2e/results`.

## The checks

`just lint` is what `check` and `web` run: `cargo fmt --check`, clippy
with warnings denied, the console's component layers
(`harness/layers.mjs`) and svelte-check. `just lex` is the `lexicons`
check. `AGENTS.md` says what each workflow runs, and when.
