#!/bin/sh
# CI-only: publish short OCI aliases X.Y.Z, X.Y and X of the Feature to
# ghcr.io/tkgstrator/commitguard. Never writes `latest`. Needs oras 1.3.4 and jq.
# Usage: ORAS_REGISTRY_CONFIG=<login file> publish-feature-alias.sh [metadata.json]
set -eu
meta=${1:-features/commitguard/devcontainer-feature.json}
src=ghcr.io/tkgstrator/commitguard/commitguard
dst=ghcr.io/tkgstrator/commitguard
mtype=application/vnd.devcontainers # Verified against the public Feature manifest.
fail() { echo "publish-feature-alias: $*" >&2; exit 1; }
[ -n "${ORAS_REGISTRY_CONFIG:-}" ] && [ -f "$ORAS_REGISTRY_CONFIG" ] || fail 'ORAS_REGISTRY_CONFIG must name a login file'
o() {
  if [ "$1" = copy ]; then
    shift
    oras copy "$@" --from-registry-config "$ORAS_REGISTRY_CONFIG" --to-registry-config "$ORAS_REGISTRY_CONFIG"
  else
    oras "$@" --registry-config "$ORAS_REGISTRY_CONFIG"
  fi
}
semver() { printf '%s\n' "$1" | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; }
# feature_version REF: print version of a Feature manifest, else fail.
feature_version() {
  m=$(o manifest fetch "$1") || fail "cannot read manifest $1"
  printf '%s' "$m" | jq -er --arg t "$mtype" '
    select(.config.mediaType == $t)
    | .annotations["dev.containers.metadata"] | fromjson
    | select(.id == "commitguard") | .version' || fail "$1 is not a commitguard Feature"
}
jq -e '.id == "commitguard"' "$meta" >/dev/null || fail 'metadata id must be commitguard'
ver=$(jq -er '.version' "$meta") && semver "$ver" || fail 'version must be canonical X.Y.Z'
minor=${ver%.*}; major=${ver%%.*}
digest=$(o resolve "$src:$ver") || fail "cannot resolve $src:$ver"
printf '%s\n' "$digest" | grep -Eq '^sha256:[0-9a-f]{64}$' || fail "bad digest $digest"
[ "$(feature_version "$src@$digest")" = "$ver" ] || fail 'source metadata version mismatch'
tags=$(o repo tags "$dst") || fail "cannot list tags of $dst"
for t in "$ver" "$minor" "$major"; do
  printf '%s\n' "$tags" | grep -Fxq "$t" || continue
  cur=$(o resolve "$dst:$t") || fail "cannot resolve $dst:$t"
  if [ "$t" = "$ver" ]; then
    [ "$cur" = "$digest" ] || fail "$dst:$t already points at $cur"
    continue
  fi
  have=$(feature_version "$dst:$t")
  semver "$have" || fail "$dst:$t has invalid version $have"
  [ "$(printf '%s\n%s\n' "$have" "$ver" | sort -V | tail -n 1)" = "$ver" ] || fail "$dst:$t is newer ($have)"
done
latest=
if printf '%s\n' "$tags" | grep -Fxq latest; then
  latest=$(o resolve "$dst:latest") || fail 'cannot resolve collection metadata'
fi
# On partial failure retain successful tags and fail. A retry is idempotent;
# do not rewind floating tags or hide registry errors with automatic rollback.
for t in "$ver" "$minor" "$major"; do
  o copy "$src@$digest" "$dst:$t" >/dev/null || fail "copy to $t failed"
  copied=$(o resolve "$dst:$t") || fail "cannot verify $t"
  [ "$copied" = "$digest" ] || fail "$t does not resolve to $digest"
done
now_tags=$(o repo tags "$dst") || fail 'cannot verify destination tags'
now=
if printf '%s\n' "$now_tags" | grep -Fxq latest; then
  now=$(o resolve "$dst:latest") || fail 'cannot verify collection metadata'
fi
[ "$now" = "$latest" ] || fail 'latest changed'
echo "published $dst:$ver $dst:$minor $dst:$major -> $digest"
