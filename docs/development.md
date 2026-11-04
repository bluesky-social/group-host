# Development

You need Rust's stable toolchain through rustup, Node at the version
`.node-version` names, `just`, and goat (github.com/bluesky-social/goat)
for the lexicons.

```sh
just setup     # the console's packages, and cargo-llvm-cov
just serve     # the host, on http://localhost:2600
just web-dev   # the console, asking that host
```

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
