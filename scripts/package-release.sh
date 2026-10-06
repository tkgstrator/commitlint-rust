#!/bin/sh
# Development packaging only; no consumer compiler or language runtime needed.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
[ -n "$version" ] || { echo 'package-release: package version missing' >&2; exit 1; }
[ -f "$root/install.sh" ] || { echo 'package-release: installer missing' >&2; exit 1; }
grep -Fqx "release_tag=v$version" "$root/install.sh" || { echo 'package-release: installer and package version differ' >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  digest() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
  digest() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
  echo 'package-release: a SHA256 utility is required' >&2; exit 1
fi
dist="$root/dist/v$version"
mkdir -p "$dist"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' 0
trap 'exit 129' 1
trap 'exit 130' 2
trap 'exit 143' 15
cp "$root/LICENSE" "$root/README.md" "$stage/"
: > "$dist/SHA256SUMS"
for target in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
  cp "$root/bin/$target/gh-commit-guard" "$root/bin/$target/commitlint" "$stage/"
  archive="commitlint-rust-$target.tar.gz"
  COPYFILE_DISABLE=1 tar -czf "$dist/$archive" -C "$stage" gh-commit-guard commitlint LICENSE README.md
  printf '%s  %s\n' "$(digest "$dist/$archive")" "$archive" >> "$dist/SHA256SUMS"
done
cp "$root/install.sh" "$dist/install.sh"
printf '%s  install.sh\n' "$(digest "$dist/install.sh")" >> "$dist/SHA256SUMS"
# Generate the Formula from the exact release archive digests.
formula="$root/Formula/commitlint-rust.rb"
cat > "$formula" <<HEADER
class CommitlintRust < Formula
  desc "Native Conventional Commits linting and gh identity guard"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "$version"
  license "MIT"
HEADER
for os in macos linux; do
  printf '\n  on_%s do\n' "$os" >> "$formula"
  for cpu in arm intel; do
    case "$os:$cpu" in
      macos:arm) target=aarch64-apple-darwin;;
      macos:intel) target=x86_64-apple-darwin;;
      linux:arm) target=aarch64-unknown-linux-musl;;
      linux:intel) target=x86_64-unknown-linux-musl;;
    esac
    archive="commitlint-rust-$target.tar.gz"
    printf '    on_%s do\n      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v%s/%s"\n      sha256 "%s"\n    end\n' "$cpu" "$version" "$archive" "$(digest "$dist/$archive")" >> "$formula"
  done
  printf '  end\n' >> "$formula"
done
cat >> "$formula" <<'RUBY'

  depends_on "gh"
  depends_on "git"

  def install
    bin.install "gh-commit-guard"
    bin.install "commitlint" => "commitlint-rust"
  end

  def caveats
    <<~EOS
      To activate user-wide Git guards and Codex/Claude skills, run:
        gh-commit-guard install
      This changes user Git and shell/agent configuration. Reopen your shell afterward.
      Finish any active history-repair task before updating its installed guard.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gh-commit-guard version")
    assert_match version.to_s, shell_output("#{bin}/commitlint-rust --version")
    assert_match "Usage:", shell_output("#{bin}/commitlint-rust --help")
  end
end
RUBY
printf 'Release assets ready: %s\n' "$dist"
