#!/bin/sh
# Whether a pull that changes what the host, its console, its MCP tools,
# the command or the lexicons do says so in a change file, as
# AGENTS.md (Versioning) asks: it adds a file under .changeset/, as `knope
# document-change` writes one, or one of its commits says on a line of
# its message
#
#   Changeset: none
#
# for a change no one running or using them sees. The pull's body would
# be the place, but a run cannot read it: it is kept in the private
# repository's space. Run by check.yml in a pull's run, at its head merged
# with its base; elsewhere it passes, since a push to main carries pulls
# checked already.

set -eu

[ -n "${KNOTLESS_RUN_PULL:-}" ] || { echo "not a pull: nothing to check"; exit 0; }
base="$KNOTLESS_RUN_BASE"
git config --global --add safe.directory '*'

changed=$(git diff --name-only "$base" HEAD -- \
  crates/group-host/src crates/group-host-mcp/src crates/opensocial/src \
  web/src web/messages lexicons/fyi)
if [ -z "$changed" ]; then
  echo "nothing anyone uses is changed: no change file needed"
  exit 0
fi
added=$(git diff --name-only --diff-filter=A "$base" HEAD -- '.changeset/*.md')
if [ -n "$added" ]; then
  echo "what people use is changed, and said so in: $added"
  exit 0
fi
if git log --format=%B "$base..HEAD" | grep -qx 'Changeset: none'; then
  echo "what people use is changed, and a commit says no change file is needed"
  exit 0
fi

echo "what people use is changed, in:"
printf "%s\n" "$changed" | sed 's/^/  /'
echo "but no change file was added under .changeset/. Add one, as \`knope document-change\` writes one (AGENTS.md, Versioning), or, if no one running or using the host sees the change, push a commit saying \`Changeset: none\` on a line of its message."
exit 1
