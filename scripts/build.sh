#!/bin/sh
# Development-only build; consumers use supplied native binaries.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$root/target"}
export CARGO_TARGET_DIR
target=${1:-$(rustc -vV | sed -n 's/^host: //p')}
case "$target" in aarch64-apple-darwin|x86_64-apple-darwin|aarch64-unknown-linux-musl|x86_64-unknown-linux-musl) ;; *) echo 'build: unsupported release target' >&2; exit 1;; esac
source_hash=$(sh "$root/scripts/source-fingerprint.sh" "$root")
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml")
if [ "${COMMITGUARD_CROSS_BUILD_ONLY:-0}" != 1 ]; then
  cargo test --workspace --locked --manifest-path "$root/Cargo.toml" --target "$target"
fi
cargo build --workspace --locked --release --manifest-path "$root/Cargo.toml" --target "$target"
[ "$(sh "$root/scripts/source-fingerprint.sh" "$root")" = "$source_hash" ] || { echo 'build: source changed during build; rebuild before packaging' >&2; exit 1; }
mkdir -p "$root/bin/$target"
cp "$CARGO_TARGET_DIR/$target/release/commitguard" "$CARGO_TARGET_DIR/$target/release/gh-commit-guard" "$CARGO_TARGET_DIR/$target/release/commitlint" "$root/bin/$target/"
if command -v sha256sum >/dev/null 2>&1; then
  digest() { value=$(sha256sum "$1") || return; printf '%s\n' "$value" | awk '{print $1}'; }
else
  digest() { value=$(shasum -a 256 "$1") || return; printf '%s\n' "$value" | awk '{print $1}'; }
fi
{
  printf 'version=%s\nsource_sha256=%s\n' "$version" "$source_hash"
  for name in commitguard gh-commit-guard commitlint; do
    hash=$(digest "$root/bin/$target/$name") || exit 1
    printf '%s  %s\n' "$hash" "$name"
  done
} > "$root/bin/$target/build-info"
manifest="$root/bin/manifest.json.tmp"
printf '{\n' > "$manifest"
separator=''
for platform in aarch64-apple-darwin aarch64-unknown-linux-musl x86_64-apple-darwin x86_64-unknown-linux-musl; do
  for name in commitguard commitlint gh-commit-guard; do
    path="$root/bin/$platform/$name"
    [ -f "$path" ] || continue
    hash=$(digest "$path")
    printf '%s  "%s/%s": "%s"' "$separator" "$platform" "$name" "$hash" >> "$manifest"
    separator=',
'
  done
done
printf '\n}\n' >> "$manifest"
mv "$manifest" "$root/bin/manifest.json"
