#!/usr/bin/env bash
# `just bootstrap`: everything the site's world runs beside it.
#
#   .deps/atproto   the spaces alpha's dev-env (the reference PDS with spaces),
#                   at the commit group-host's CI tests against
#                   (harness/alpha.sh), kept out of git
#
# Then the packages: the group host's console and the SDK it builds on, the
# site's, and the harness's. The group host itself is this repository,
# built by `just host` before each world. The dev-env is built once, and
# again only when alpha.sh's commit changes. ATPROTO_DEV_ENV, set, names a
# built dev-env to use instead of building one (another checkout's .deps, say).
#
# Needs git, Rust (rustup), and Node at .node-version's version with corepack.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$ROOT/../.." && pwd)"
DEPS="${KICKER_DEPS:-$ROOT/.deps}"

say() { printf '\n== %s\n' "$*"; }
fail() { printf 'bootstrap: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null || fail "$1 is needed: $2"; }

# The space alpha group-host's CI tests against.
ATPROTO_COMMIT="$(sed -n 's/^ATPROTO_COMMIT=//p' "$REPO/harness/alpha.sh")"
[ -n "$ATPROTO_COMMIT" ] || fail "no ATPROTO_COMMIT in $REPO/harness/alpha.sh"

need git "https://git-scm.com"
need rustup "https://rustup.rs"
need node "Node $(cat "$REPO/.node-version")"
need corepack "it comes with Node"
[ "$(node -p 'process.versions.node')" = "$(cat "$REPO/.node-version")" ] ||
  fail "Node $(cat "$REPO/.node-version") is needed, not $(node -p 'process.versions.node')"

if [ -n "${ATPROTO_DEV_ENV:-}" ]; then
  say "the spaces alpha's dev-env, from $ATPROTO_DEV_ENV"
else
  say "the spaces alpha's dev-env, at ${ATPROTO_COMMIT:0:7}"
  A="$DEPS/atproto"
  if [ "$(cat "$A/.built" 2>/dev/null)" = "$ATPROTO_COMMIT" ]; then echo "built already"; else
    if [ ! -d "$A/.git" ]; then
      mkdir -p "$A"
      git -C "$A" init -q
      git -C "$A" remote add origin https://github.com/bluesky-social/atproto.git
    fi
    git -C "$A" fetch -q --depth 1 origin "$ATPROTO_COMMIT" 2>/dev/null || git -C "$A" fetch -q origin
    git -C "$A" checkout -q --force "$ATPROTO_COMMIT"
    git -C "$A" clean -qfd
    (
      cd "$A"
      export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
      corepack pnpm install --frozen-lockfile --ignore-scripts
      corepack pnpm run build:tooling --force
      corepack pnpm --filter '@atproto/dev-env^...' --filter @atproto/dev-env run --if-present prebuild
      corepack pnpm exec tsc --build packages/dev-env/tsconfig.build.json
      # The PDS serves its sign-in pages from this bundle, and will not start without it.
      corepack pnpm --filter @atproto/oauth-provider-ui run build:ui
      # SQLite's native module, which the install above skips building.
      sqlite="$(dirname "$(node -p "require.resolve('better-sqlite3/package.json', { paths: ['$A/packages/pds'] })")")"
      [ -f "$sqlite/build/Release/better_sqlite3.node" ] || (cd "$sqlite" && npm run install)
    )
    echo "$ATPROTO_COMMIT" >"$A/.built"
  fi
fi

say "the packages: the console's, the SDK's, the site's and the harness's"
(cd "$REPO/packages/sdk" && npm ci --no-audit --no-fund)
(cd "$REPO/web" && npm ci --no-audit --no-fund)
(cd "$ROOT/site" && npm ci --no-audit --no-fund)
(cd "$ROOT/harness" && npm ci --no-audit --no-fund)

say "done"
echo "Run the site with \`just dev\`, its flows with \`just e2e\`."
