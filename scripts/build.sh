#!/bin/sh
# Development-only build; consumers use supplied native binaries.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$root/target"}
export CARGO_TARGET_DIR
target=${1:-$(rustc -vV | sed -n 's/^host: //p')}
cargo test --locked --manifest-path "$root/Cargo.toml" --target "$target"
cargo build --locked --release --manifest-path "$root/Cargo.toml" --target "$target"
mkdir -p "$root/bin/$target"
cp "$CARGO_TARGET_DIR/$target/release/gh-commit-guard" "$CARGO_TARGET_DIR/$target/release/commitlint" "$root/bin/$target/"
