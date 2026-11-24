# The stable toolchain's own binaries first: cargo finds its clippy and
# rustfmt on the PATH, where another install's (Homebrew's) can come first
# and lint with an older clippy than the check workflow's.
toolchain := `dirname "$(rustup which --toolchain stable cargo)"`
cargo := "env PATH='" + toolchain + ":" + env_var('PATH') + "' rustup run stable cargo"
ignore := "(tests/|harness/)"
# oxfmt, from the console's packages, and what it formats.
oxfmt := "web/node_modules/.bin/oxfmt"
js := "web harness docs-site"

# Install the web pages' packages, the harness's (the reference PDS the
# tests reach the network through), and coverage tools.
setup:
    cd web && npm install --no-audit --no-fund
    cd harness && npm ci --no-audit --no-fund
    rustup component add llvm-tools-preview --toolchain stable
    command -v cargo-llvm-cov >/dev/null || cargo install cargo-llvm-cov

test:
    {{cargo}} test --workspace

# Tests while working: only those `args` pick, as cargo test takes them.
t *args:
    {{cargo}} test {{args}}

# Fails under 100% of lines or functions, as the test check does.
cov:
    {{cargo}} llvm-cov --workspace --ignore-filename-regex '{{ignore}}' --fail-under-lines 100 --fail-under-functions 100 --show-missing-lines

cov-html:
    {{cargo}} llvm-cov --workspace --ignore-filename-regex '{{ignore}}' --html --open

# The lexicons: goat's lint (github.com/bluesky-social/goat), what it does
# not see, and that they only grow from main's.
lex:
    node harness/lexicons.mjs

# The docs' links and anchors, as the docs check runs it.
docs:
    node harness/docs.mjs

# The docs as their site, at http://localhost:4321/, redrawn as they change.
docs-site:
    cd docs-site && npm ci --no-audit --no-fund && npm run dev

# An app password cannot write the space a private repository's site is
# kept in, so this runs as its owner, signed in with `knotless login`.
# The docs site, built and deployed as the repository's, for its members.
docs-deploy:
    cd docs-site && npm ci --no-audit --no-fund && npm run build
    knotless site knotless://chadtmiller.com/group-host docs-site/dist --not-found 404.html

# Tags the release that last landed, then cuts the next from main's
# change files as the `Release <version>` pull (deploy/release.sh), as
# the owner, signed in with `knotless login`; --dry-run pushes nothing.
release *args:
    deploy/release.sh {{args}}

lint:
    {{cargo}} fmt --all --check
    {{oxfmt}} --check {{js}}
    {{cargo}} clippy --workspace --all-targets -- -D warnings
    {{cargo}} deny check
    node harness/layers.mjs
    node harness/dialogs.mjs
    node harness/strings.mjs
    cd web && npm run check

# Formats the Rust (rustfmt) and the console's, the harness's
# and the docs site's code (oxfmt, by .oxfmtrc.json), as `lint` and the
# checks expect.
fmt:
    {{cargo}} fmt --all
    {{oxfmt}} {{js}}

# The host, on PORT (2600 by default): docs/reference/config.md.
serve *args:
    {{cargo}} run -p group-host -- {{args}}

# The console, built into web/build, its types from the lexicons.
web:
    cd web && npm run build

# The console, redrawn as its source changes, asking a `just serve` on its
# default port.
web-dev:
    cd web && npm run dev

# The console to use yourself, over a world of its own (harness/dev.mjs):
# the reference PDS with alice, bob and harbor, the host, and the console
# at https://group-host.localtest.me, behind portless, redrawn as its
# source changes; Riders, alice's, and Hikers, bob's, inviting her. Empty
# each time; the host and the command are built again first.
dev:
    cd web && npm run lex
    {{cargo}} build -p group-host -p opensocial
    node harness/dev.mjs

# The console used as people use it (harness/e2e/flows.spec.mjs): in
# Chromium, signed in to through the reference PDS's own approval page,
# and an app signed in as a group through it, over a world of its own.
e2e *args: web
    {{cargo}} build -p group-host
    cd harness && npx playwright test -c e2e/playwright.config.mjs {{args}}

# The guides' pictures, drawn again from the look line above each
# (harness/shots.mjs), each guide over a fresh world of its own: the
# guides named, or all of them. No `look-up` may be running.
[positional-arguments]
shots *guides:
    {{cargo}} build -p group-host -p opensocial
    cd web && npm run lex
    node harness/shots.mjs "$@"

# The console's pages as pictures in target/look, to open: any path, in
# any state, signed in as anyone, on a phone or a desktop, light or dark,
# a dialog open (harness/look/look.mjs, whose top says every option):
# `just look /g/riders.groups.test --as alice`, `just look / --seed full
# --as bob --click 'button[popovertarget=invite]'`. Drawn over a world of
# its own, the console from its source as it is now, or over the one
# `look-up` keeps.
[positional-arguments]
look *args:
    {{cargo}} build -p group-host -p opensocial
    cd web && npm run lex
    node harness/look/look.mjs "$@"

# A world for `just look` to draw over, kept up until ctrl-c: each look
# then takes seconds, not the half minute starting one does.
look-up:
    {{cargo}} build -p group-host -p opensocial
    cd web && npm run lex
    node harness/look/look.mjs --up

# The opensocial command.
opensocial *args:
    {{cargo}} run -q -p opensocial -- {{args}}
