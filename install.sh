#!/bin/sh
# Clone-free one-run installer: downloads the verified native release with Git+gh
# and runs `gh-commit-guard install "$@"`. All arguments go to the native installer.
set -eu
umask 077

repo=github.com/tkgstrator/commitlint-rust
release_tag=v0.2.0

die() { echo "install.sh: $*" >&2; exit 1; }

# COMMITLINT_RUST_VERSION only selects the release; it is never a native option.
tag=${COMMITLINT_RUST_VERSION:-$release_tag}
case "$tag" in
  v[0-9A-Za-z]*) ;;
  *) die "COMMITLINT_RUST_VERSION must be a v-prefixed tag" ;;
esac
case "$tag" in
  *[!0-9A-Za-z._+-]*) die "COMMITLINT_RUST_VERSION contains unsafe characters" ;;
esac

# Same platform map as scripts/check; fail before any download.
case "$(uname -s):$(uname -m)" in
  Darwin:arm64) target=aarch64-apple-darwin ;;
  Darwin:x86_64) target=x86_64-apple-darwin ;;
  Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
  Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-musl ;;
  *) die "no native release for this platform" ;;
esac

for tool in git gh tar gzip mktemp awk sort wc tr find mkdir chmod rm uname; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { hash_output=$(sha256sum "$1") || return; printf '%s\n' "$hash_output" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { hash_output=$(shasum -a 256 "$1") || return; printf '%s\n' "$hash_output" | awk '{print $1}'; }
else
  die "sha256sum or shasum is required"
fi

unset GH_DEBUG DEBUG
GH_PROMPT_DISABLED=1
GH_NO_UPDATE_NOTIFIER=1
export GH_PROMPT_DISABLED GH_NO_UPDATE_NOTIFIER

tmp=$(mktemp -d "${TMPDIR:-/tmp}/commitlint-rust.XXXXXX") || die "cannot create temporary directory"
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

archive="commitlint-rust-$target.tar.gz"
dl="$tmp/download"
out="$tmp/payload"
mkdir "$dl" "$out"

gh release download "$tag" --repo "$repo" --pattern "$archive" --pattern SHA256SUMS --dir "$dl" \
  || die "release download failed ($repo $tag)"
[ -f "$dl/$archive" ] && [ -f "$dl/SHA256SUMS" ] || die "release assets missing"

# Exactly one lowercase 64-hex digest entry for the selected archive.
entries=$(awk -v n="$archive" '$2 == n || $2 == "*" n { if (NF != 2) print "invalid-entry"; else print $1 }' "$dl/SHA256SUMS")
case "$entries" in
  "" | *"
"*) die "SHA256SUMS must contain exactly one entry for $archive" ;;
esac
case "$entries" in
  *[!0-9a-f]*) die "invalid digest in SHA256SUMS" ;;
esac
[ "${#entries}" -eq 64 ] || die "invalid digest length in SHA256SUMS"
actual=$(sha256 "$dl/$archive")
[ "$actual" = "$entries" ] || die "checksum mismatch for $archive"

# Validate the fixed payload before extracting or executing anything.
expected="LICENSE
README.md
commitlint
gh-commit-guard"
names=$(tar -tzf "$dl/$archive") || die "cannot list archive"
[ "$(printf '%s\n' "$names" | LC_ALL=C sort)" = "$expected" ] \
  || die "archive must contain exactly: gh-commit-guard commitlint LICENSE README.md"
types=$(tar -tvzf "$dl/$archive") || die "cannot list archive"
[ "$(printf '%s\n' "$types" | wc -l | tr -d ' ')" -eq 4 ] || die "unexpected archive listing"
printf '%s\n' "$types" | while IFS= read -r line; do
  case "$line" in
    -*) ;;
    *) exit 1 ;;
  esac
done || die "archive entries must be regular files"

tar -xzf "$dl/$archive" -C "$out" || die "extraction failed"
for f in gh-commit-guard commitlint LICENSE README.md; do
  [ -f "$out/$f" ] && [ ! -L "$out/$f" ] || die "unexpected extracted entry: $f"
done
[ "$(find "$out" -mindepth 1 | wc -l | tr -d ' ')" -eq 4 ] || die "unexpected extracted files"
chmod 0755 "$out/gh-commit-guard"

echo "install.sh: running native commit policy and agent skill setup" >&2
"$out/gh-commit-guard" install "$@"
