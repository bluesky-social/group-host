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

## The checks

`just lint` is what `check` and `web` run: `cargo fmt --check`, clippy
with warnings denied, the console's component layers
(`harness/layers.mjs`) and svelte-check. `just lex` is the `lexicons`
check. `AGENTS.md` says what each workflow runs, and when.
