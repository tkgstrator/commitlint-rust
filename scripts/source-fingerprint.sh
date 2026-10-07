#!/bin/sh
# Development build provenance; never executed by the installed guard.
set -eu
root=${1:-$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)}
cd "$root"
if command -v sha256sum >/dev/null 2>&1; then
  digest() { value=$(sha256sum "$@") || return; printf '%s\n' "$value" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  digest() { value=$(shasum -a 256 "$@") || return; printf '%s\n' "$value" | awk '{print $1}'; }
else
  echo 'source-fingerprint: SHA256 utility missing' >&2; exit 1
fi
found=$(find crates resources -type f ! -path '*/node_modules/*' ! -path '*/target/*')
files=$(printf '%s\n' "$found" | LC_ALL=C sort)
records=$(
  for f in Cargo.toml Cargo.lock README.md LICENSE install.sh scripts/build.sh scripts/package-release.sh scripts/source-fingerprint.sh scripts/check scripts/commitlint; do
    hash=$(digest "$f") || exit 1
    printf '%s  %s\n' "$hash" "$f" || exit 1
  done
  printf '%s\n' "$files" | while IFS= read -r f; do
    hash=$(digest "$f") || exit 1
    printf '%s  %s\n' "$hash" "$f"
  done
)
printf '%s\n' "$records" | digest
