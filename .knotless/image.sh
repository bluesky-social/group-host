#!/bin/sh
# The host's image as an OCI image layout, made with no container engine,
# as release.yml's runner gives its jobs none. It is the Dockerfile's last
# stage: node:24.22.1-bookworm-slim for linux/amd64, as the registry has it
# when this runs, and one layer more holding the host, the command, the
# console, tini and the CA bundle, with the Dockerfile's user, environment,
# working directory, entry point and command. The image's labels, and its
# manifest's annotations, name its version, the commit it was built from
# and the base, by digest.
#
#   SOURCE_DATE_EPOCH=<commit time> TINI=<tini-static> \
#     .knotless/image.sh <bins> <console> <version> <commit> <dir>
#
# <bins> holds group-host and atgrp, built for linux/amd64 against
# bookworm's glibc; <console> is web/, built (its build/ and package.json).
# The layer's files and the image's dates are SOURCE_DATE_EPOCH's, so the
# same build on the same base makes the same image. Needs crane, jq, GNU
# tar, gzip and sha256sum.
set -eu

bins=$1 console=$2 version=$3 commit=$4 out=$5
: "${SOURCE_DATE_EPOCH:?the commit's time, in seconds}"
: "${TINI:?a static tini, as the Dockerfile's apt-get installs it}"
base=node:24.22.1-bookworm-slim
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

blob() { echo "$work/base/blobs/sha256/${1#sha256:}"; }
digest=$(crane digest --platform linux/amd64 "$base")
crane pull --format=oci "$base@$digest" "$work/base"
manifest=$(blob "$(jq -r '.manifests[0].digest' "$work/base/index.json")")
jq -e '.mediaType == "application/vnd.oci.image.manifest.v1+json"
  and all(.layers[]; .mediaType == "application/vnd.oci.image.layer.v1.tar+gzip")' \
  "$manifest" >/dev/null || { echo "$base@$digest: not an OCI image of gzipped layers" >&2; exit 1; }

# The layer: what the Dockerfile's last stage adds to the base. The base
# has the node user (1000) the image runs as, who owns /data.
root="$work/root"
mkdir -p "$root/usr/local/bin" "$root/usr/bin" "$root/etc/ssl/certs" "$root/app" "$root/data"
install -m 755 "$bins/group-host" "$bins/atgrp" "$root/usr/local/bin/"
install -m 755 "$TINI" "$root/usr/bin/tini"
install -m 644 /etc/ssl/certs/ca-certificates.crt "$root/etc/ssl/certs/"
cp -R "$console/build" "$root/app/build"
install -m 644 "$console/package.json" "$root/app/package.json"
chown -R 0:0 "$root"
chown 1000:1000 "$root/data"
tar -C "$root" -cf "$work/layer.tar" --sort=name --mtime="@$SOURCE_DATE_EPOCH" \
  --numeric-owner --format=posix --pax-option=delete=atime,delete=ctime app data etc usr
diff_id=$(sha256sum "$work/layer.tar" | cut -d' ' -f1)
gzip -9n < "$work/layer.tar" > "$work/layer.tar.gz"

created=$(date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%dT%H:%M:%SZ)
jq --arg diff "sha256:$diff_id" --arg created "$created" \
  --arg version "$version" --arg commit "$commit" --arg base "docker.io/library/$base" --arg digest "$digest" '
  .created = $created
  | .rootfs.diff_ids += [$diff]
  | .history = (.history // []) + [{created: $created, created_by: "the host, the command and the console (.knotless/image.sh)"}]
  | .config.User = "node"
  | .config.Env = (.config.Env // []) + ["DATA_DIR=/data", "PORT=2600"]
  | .config.WorkingDir = "/app"
  | .config.Entrypoint = ["tini", "--"]
  | .config.Cmd = ["group-host"]
  | .config.Labels = (.config.Labels // {}) + {
      "org.opencontainers.image.title": "group-host",
      "org.opencontainers.image.licenses": "MIT OR Apache-2.0",
      "org.opencontainers.image.version": $version,
      "org.opencontainers.image.revision": $commit,
      "org.opencontainers.image.source": "https://knotless.dev/chadtmiller.com/group-host",
      "org.opencontainers.image.base.name": $base,
      "org.opencontainers.image.base.digest": $digest
    }' "$(blob "$(jq -r '.config.digest' "$manifest")")" > "$work/config.json"

# The layout: the base's layers, the new one, the config, the manifest and
# an index naming it alone.
rm -rf "$out"
mkdir -p "$out/blobs/sha256"
put() {
  d=$(sha256sum "$1" | cut -d' ' -f1)
  cp "$1" "$out/blobs/sha256/$d"
  echo "$d $(wc -c < "$1" | tr -d ' ')"
}
for layer in $(jq -r '.layers[].digest' "$manifest"); do cp "$(blob "$layer")" "$out/blobs/sha256/"; done
read -r layer_d layer_n <<EOF
$(put "$work/layer.tar.gz")
EOF
read -r config_d config_n <<EOF
$(put "$work/config.json")
EOF
jq --arg ld "sha256:$layer_d" --argjson ln "$layer_n" --arg cd "sha256:$config_d" --argjson cn "$config_n" \
  --arg version "$version" --arg commit "$commit" --arg base "docker.io/library/$base" --arg digest "$digest" '
  .config = {mediaType: "application/vnd.oci.image.config.v1+json", digest: $cd, size: $cn}
  | .layers += [{mediaType: "application/vnd.oci.image.layer.v1.tar+gzip", digest: $ld, size: $ln}]
  | .annotations = {
      "org.opencontainers.image.version": $version,
      "org.opencontainers.image.revision": $commit,
      "org.opencontainers.image.base.name": $base,
      "org.opencontainers.image.base.digest": $digest
    }' "$manifest" > "$work/manifest.json"
read -r manifest_d manifest_n <<EOF
$(put "$work/manifest.json")
EOF
jq -n --arg d "sha256:$manifest_d" --argjson n "$manifest_n" --arg version "$version" '{
  schemaVersion: 2,
  mediaType: "application/vnd.oci.image.index.v1+json",
  manifests: [{
    mediaType: "application/vnd.oci.image.manifest.v1+json", digest: $d, size: $n,
    platform: {architecture: "amd64", os: "linux"},
    annotations: {"org.opencontainers.image.ref.name": $version}
  }]
}' > "$out/index.json"
echo '{"imageLayoutVersion":"1.0.0"}' > "$out/oci-layout"
echo "$out: group-host $version at $commit on $base@$digest, manifest sha256:$manifest_d"
