#!/usr/bin/env bash
# Releases the workspace from main's change files, as the repository's
# owner, signed in with `knotless login`:
#
#   deploy/release.sh [--dry-run]
#
# First it tags the release that last landed: when main's version has a
# commit on main titled `Release <version>` and no tag `v<version>`, it
# tags that commit, which mirror.yml then pushes to GitHub. Then, when
# main has change files, it cuts the next release at main's tip, in a
# worktree of its own: `knope release --override-version <version>`,
# committed as `Release <version>`, force-pushed to release/next, and the
# pull of that title opened, or the open one brought up to date. Landing
# that pull is the release; run this again once it has, to tag it.
#
# The version is worked out here, not by knope: a change file's `minor` or
# `major` bumps 0.x's minor, where knope would bump the patch, and a
# `patch` the patch; from 1.0 on, each bumps its own part. A pull already
# holding main's change files, and still merging with main, is left as it
# is, so its checks run again only when what it releases changes.
#
# With --dry-run it tags nothing, and makes the release commit and prints
# the pull's body, pushing nothing.

set -euo pipefail
cd "$(dirname "$0")/.."

REPO=knotless://chadtmiller.com/group-host
OWNER=chadtmiller.com
BRANCH=release/next
dry=
case "${1:-}" in
  --dry-run) dry=1 ;;
  "") ;;
  *) sed -n '5p' "$0" >&2; exit 2 ;;
esac
command -v knope >/dev/null || { echo "needs knope (https://knope.tech): cargo install knope" >&2; exit 1; }

root=$PWD
git fetch -q "$REPO" main
main=$(git rev-parse FETCH_HEAD)
current=$(git show "$main:Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)

# The release that landed last, tagged once. awk reads all it is given,
# the first match kept: quitting early would end the writer with SIGPIPE,
# which pipefail takes for a failure.
landed=$(git log --format='%H%x09%s' "$main" | awk -F '\t' -v t="Release $current" '$2 == t && !n++ { print $1 }')
if [ -z "$landed" ]; then
  echo "main's $current came from no release pull: nothing to tag"
else
  # Exit 2 alone says there is no such tag; any other failure says nothing.
  asked=0
  git ls-remote --exit-code -q "$REPO" "refs/tags/v$current" >/dev/null || asked=$?
  case $asked in
    0) echo "v$current is tagged already" ;;
    2)
      if [ -n "$dry" ]; then
        echo "would tag $landed v$current"
      else
        knotless tag create "$REPO" "v$current" "$landed"
        echo "tagged $landed v$current"
      fi
      ;;
    *) echo "could not ask $REPO whether v$current is tagged" >&2; exit 1 ;;
  esac
fi

tree=$(mktemp -d)
trap 'git -C "$root" worktree remove --force "$tree"' EXIT
git worktree add -q --detach "$tree" "$main"
cd "$tree"

set -- .changeset/*.md
if [ ! -e "$1" ]; then
  echo "no change files on main: nothing to release"
  exit 0
fi
count=$#
bumps=$(sed -n 's/^default: *//p' "$@")
IFS=. read -r major minor patch <<< "$current"
if echo "$bumps" | grep -qx major && [ "$major" != 0 ]; then
  version=$((major + 1)).0.0
elif echo "$bumps" | grep -qxE 'major|minor'; then
  version=$major.$((minor + 1)).0
else
  version=$major.$minor.$((patch + 1))
fi

knope release --override-version "$version"
git commit -aqm "Release $version"
at=$(echo "$main" | cut -c1-12)
notes=$(awk -v v="$version" '/^## / { on = ($2 == v); next } on' CHANGELOG.md)
body=$(mktemp)
cat > "$body" <<EOF
Releases group-host $version, from the $count change files on main: its changelog is below. Merging this pull releases it; \`just release\` then tags it \`v$version\`, and brings this pull up to date as more change files land.

<details>
<summary>deploy/release.sh</summary>

## What changes

group-host $version, as \`knope release --override-version $version\` makes it from main at $at: the workspace version from $current to $version in Cargo.toml and the crates' Cargo.lock entries, which \`/xrpc/_health\` and the MCP server's \`serverInfo\` name once it is deployed, the version's CHANGELOG.md section, and the change files removed.

$notes

## Why

A version names what the host, the console and the command do for those who run and use them. A 0.x minor change bumps the minor here, where knope alone would bump the patch.

## How it was checked

The build and tests are left to CI.

</details>
EOF

if [ -n "$dry" ]; then
  cat "$body"
  exit 0
fi

pull=$(knotless pull list "$REPO" -q "is:pull state:open author:$OWNER Release" |
  awk -F '\t' '$4 ~ /^Release [0-9]+\.[0-9]+\.[0-9]+$/ && !n++ { print $3 }')
if [ -n "$pull" ] && git fetch -q "$REPO" "refs/heads/$BRANCH" &&
  [ "$(git rev-parse "FETCH_HEAD^:.changeset")" = "$(git rev-parse "$main:.changeset")" ] &&
  git merge-tree --write-tree "$main" FETCH_HEAD > /dev/null; then
  echo "$pull releases main's change files already and merges with main: nothing to push"
  exit 0
fi
git push -q --force "$REPO" "HEAD:refs/heads/$BRANCH"
if [ -z "$pull" ]; then
  knotless pull create "$REPO" main "$REPO" "$BRANCH" "Release $version" -m - < "$body"
else
  knotless pull retarget "$REPO" "$pull" --head "$BRANCH"
  knotless pull edit "$REPO" "$pull" --title "Release $version" -m - < "$body"
  echo "updated $pull"
fi
