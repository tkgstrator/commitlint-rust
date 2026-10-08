#!/bin/sh
# Build the real local Feature through the pinned CLI and test isolated runtime.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
base_image=${1:-mcr.microsoft.com/devcontainers/base:ubuntu-24.04}
cli=${DEVCONTAINER_CLI:-devcontainer}
tmp=$(mktemp -d)
nonce=$(date +%s)-$$
image=commitguard-feature-test:$nonce
container=commitguard-feature-test-$nonce
builder=commitguard-feature-builder-$nonce
# Every resource belongs to this invocation. Never prune shared Docker resources.
cleanup() {
  docker rm -f "$container" >/dev/null 2>&1 || true
  docker image rm "$image" >/dev/null 2>&1 || true
  docker buildx rm "$builder" >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$tmp/project/.devcontainer/features/after-guard"
cp -R "$repo/features/commitguard" "$tmp/project/.devcontainer/features/"
printf '%s\n' '{"id":"after-guard","name":"Build regression","version":"1.0.0","dependsOn":{"./features/commitguard":{}}}' > "$tmp/project/.devcontainer/features/after-guard/devcontainer-feature.json"
printf '%s\n' '#!/bin/sh' 'set -e' 'git --version' > "$tmp/project/.devcontainer/features/after-guard/install.sh"
chmod +x "$tmp/project/.devcontainer/features/after-guard/install.sh"
# Fixed supported base names avoid interpolating arbitrary JSON/shell values.
case "$base_image" in
  mcr.microsoft.com/devcontainers/base:ubuntu-24.04|mcr.microsoft.com/devcontainers/base:debian) ;;
  *) echo 'unsupported test base image' >&2; exit 1 ;;
esac
printf '{"image":"%s","remoteUser":"vscode","features":{"./features/commitguard":{},"./features/after-guard":{}}}\n' "$base_image" > "$tmp/project/.devcontainer/devcontainer.json"
docker buildx create --name "$builder" --driver docker-container
BUILDX_BUILDER=$builder
export BUILDX_BUILDER
"$cli" build --oci-auth-hardening --workspace-folder "$tmp/project" --image-name "$image" --output "type=docker,dest=$tmp/image.tar"
[ -s "$tmp/image.tar" ]
docker image load --input "$tmp/image.tar"
for user in vscode root; do
  docker run --rm --name "$container" --user "$user" --entrypoint /bin/sh \
    --mount "type=bind,src=$repo/tests/devcontainer-feature,dst=/test,readonly" \
    "$image" /test/runtime.sh
done
docker run --rm --name "$container" --user root --entrypoint /bin/sh \
  --mount "type=bind,src=$repo/tests/devcontainer-feature,dst=/test,readonly" \
  --mount "type=bind,src=$repo/features,dst=/source,readonly" \
  "$image" /test/download.sh
# A root-owned mounted HOME reaches the actual mount check on Linux as well.
docker run --rm --name "$container" --user root --entrypoint /bin/sh \
  --mount type=tmpfs,dst=/shared-home --env HOME=/shared-home "$image" -c \
  'printf "retain\n" > /shared-home/sentinel; mkdir /tmp/mount-test; /usr/bin/git -C /tmp/mount-test init -q; cd /tmp/mount-test; /usr/local/share/commitguard/setup --auto > /tmp/error 2>&1 && exit 1; grep -q "container-private HOME" /tmp/error; test "$(cat /shared-home/sentinel)" = retain; test "$(find /shared-home -type f | wc -l)" = 1'
echo 'PASS shared HOME rejected before mutation'
