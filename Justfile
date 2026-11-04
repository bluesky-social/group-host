# The stable toolchain's own binaries first: cargo finds its clippy and
# rustfmt on the PATH, where another install's (Homebrew's) can come first
# and lint with an older clippy than the check workflow's.
toolchain := `dirname "$(rustup which --toolchain stable cargo)"`
cargo := "env PATH='" + toolchain + ":" + env_var('PATH') + "' rustup run stable cargo"
ignore := "(tests/|harness/)"

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

lint:
    {{cargo}} fmt --all --check
    {{cargo}} clippy --workspace --all-targets -- -D warnings
    node harness/layers.mjs
    cd web && npm run check

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
# at https://group-host.localtest.me, behind portless. Empty each time.
dev: web
    {{cargo}} build -p group-host
    node harness/dev.mjs

# The console used as people use it (harness/e2e/flows.spec.mjs): in
# Chromium, signed in to through the reference PDS's own approval page,
# and an app signed in as a group through it, over a world of its own.
e2e *args: web
    {{cargo}} build -p group-host
    cd harness && npx playwright test -c e2e/playwright.config.mjs {{args}}

# The opensocial command.
opensocial *args:
    {{cargo}} run -q -p opensocial -- {{args}}
