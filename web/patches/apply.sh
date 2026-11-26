#!/bin/sh
# Patches to packages as npm installs them, each a git patch named for the
# package and version it fixes, applied once after each install (the
# `postinstall` script). Each goes once upstream publishes its fix; a
# version it no longer fits fails the install, so it isn't kept by mistake.
#
# - @atproto/lex-schema's `l.space()` (the spaces alpha of November 24)
#   infers one record schema for all of a space's collections, so a space
#   listing records of different types or keys, as the meta and members
#   spaces do, fails svelte-check. Patched, it infers each and gives the
#   space their union. Its types alone: nothing that runs changes.
#
# The console's image installs without this directory and never
# type-checks, so it needs none of them.
set -eu
cd "$(dirname "$0")/.."
# git apply takes a patch's paths from the repository's top when run in
# one, and from here when not (the image has no .git).
prefix=$(git rev-parse --show-prefix 2>/dev/null || true)
dir="${prefix}node_modules/@atproto/lex-schema"
patch=patches/@atproto+lex-schema+0.0.0-spaces-alpha-20261124235736.patch
if git apply -R --check --directory="$dir" "$patch" 2>/dev/null; then
  exit 0
fi
git apply --directory="$dir" "$patch"
