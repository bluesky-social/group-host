#!/bin/sh
# Whether a pull that changes what the host, its console, its MCP tools,
# the command or the lexicons do says so in a change file, as
# AGENTS.md (Versioning) asks: it adds a file under .changeset/, as `knope
# document-change` writes one, or its body says on a line of its own
#
#   Changeset: none
#
# for a change no one running or using them sees. The body is read from
# the pull's record on its author's PDS, as it stands when this runs: a
# body changed later is read once `check` runs again. Run by check.yml in
# a pull's run, at its head merged with its base; elsewhere it passes,
# since a push to main carries pulls checked already.

set -eu

[ -n "${KNOTLESS_RUN_PULL:-}" ] || { echo "not a pull: nothing to check"; exit 0; }
base="$KNOTLESS_RUN_BASE"
git config --global --add safe.directory '*'

changed=$(git diff --name-only "$base" HEAD -- \
  crates/group-host/src crates/group-host-mcp/src crates/atgrp/src \
  web/src web/messages lexicons/org)
if [ -z "$changed" ]; then
  echo "nothing anyone uses is changed: no change file needed"
  exit 0
fi
added=$(git diff --name-only --diff-filter=A "$base" HEAD -- '.changeset/*.md')
if [ -n "$added" ]; then
  echo "what people use is changed, and said so in: $added"
  exit 0
fi

# The pull, at://<author>/dev.knotless.pull/<rkey>, read from the PDS its
# author's DID document names.
author=${KNOTLESS_RUN_PULL#at://}
author=${author%%/*}
rkey=${KNOTLESS_RUN_PULL##*/}
case $author in
  did:plc:*) doc=https://plc.directory/$author ;;
  did:web:*) doc=https://${author#did:web:}/.well-known/did.json ;;
  *) echo "the pull's author, $author, is no did:plc or did:web" >&2; exit 1 ;;
esac
command -v jq >/dev/null || { apt-get update -qq && apt-get install -y -qq --no-install-recommends jq >/dev/null; }
pds=$(curl -fsS "$doc" | jq -r '.service[] | select(.id | endswith("#atproto_pds")) | .serviceEndpoint')
record=$(curl -fsS "$pds/xrpc/com.atproto.repo.getRecord?repo=$author&collection=dev.knotless.pull&rkey=$rkey")
if printf '%s\n' "$record" | jq -r '.value.body // ""' | grep -qx 'Changeset: none'; then
  echo "what people use is changed, and the pull's body says no change file is needed"
  exit 0
fi

echo "what people use is changed, in:"
printf "%s\n" "$changed" | sed 's/^/  /'
echo "but no change file was added under .changeset/. Add one, as \`knope document-change\` writes one (AGENTS.md, Versioning), or, if no one running or using the host sees the change, say \`Changeset: none\` on a line of the pull's body, then run this check again: knotless run rerun knotless://$KNOTLESS_RUN_REPO --pull $KNOTLESS_RUN_PULL --workflow check"
exit 1
