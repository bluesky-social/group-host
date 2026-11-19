#!/bin/sh
# Builds the space alpha's dev-env at /opt/atproto, for the reference PDS
# the `test` and `e2e` checks run on (GROUP_HOST_DEV_ENV, harness/pds.mjs):
# permissioned-data-alpha as of November 13, 2026, the build
# @atproto/space 0.0.0-spaces-alpha-20261113224550 was published from.
# A build kept from an earlier run is used again while ATPROTO_COMMIT and
# BUILD are what it was made at; BUILD is bumped when these steps change.
# Then SQLite's native module for its PDS, which the install skips.
set -eu
ATPROTO_COMMIT=83d473dec69bbf19fe95fcbbb20a39842779db82
BUILD=2
dir=/opt/atproto
export PATH="/opt/node/bin:$dir/.bin:$PATH" COREPACK_ENABLE_DOWNLOAD_PROMPT=0 COREPACK_HOME="$dir/.corepack"

if [ "$(cat "$dir/.built" 2>/dev/null)" != "$ATPROTO_COMMIT $BUILD" ]; then
  mkdir -p "$dir"
  find "$dir" -mindepth 1 -delete
  cd "$dir"
  git init -q . && git fetch -q --depth 1 https://github.com/bluesky-social/atproto.git "$ATPROTO_COMMIT" && git checkout -q FETCH_HEAD
  mkdir .bin && corepack enable --install-directory "$dir/.bin"
  pnpm install --frozen-lockfile --ignore-scripts
  pnpm run build:tooling --force
  pnpm --filter '@atproto/dev-env^...' --filter @atproto/dev-env run --if-present prebuild
  pnpm exec tsc --build packages/dev-env/tsconfig.build.json
  # The PDS serves its OAuth sign-in pages from this bundle, and will not
  # start without it.
  pnpm --filter @atproto/oauth-provider-ui run build:ui
  echo "$ATPROTO_COMMIT $BUILD" > "$dir/.built"
fi

sqlite="$(dirname "$(node -p "require.resolve('better-sqlite3/package.json', { paths: ['$dir/packages/pds'] })")")"
[ -f "$sqlite/build/Release/better_sqlite3.node" ] || (cd "$sqlite" && npm run install)
