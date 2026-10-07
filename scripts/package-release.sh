#!/bin/sh
# Development packaging; installed runtime never invokes Cargo or this script.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml")
[ -n "$version" ] || { echo 'package-release: workspace version missing' >&2; exit 1; }
[ -f "$root/install.sh" ] || { echo 'package-release: installer missing' >&2; exit 1; }
grep -Fqx "release_tag=v$version" "$root/install.sh" || { echo 'package-release: installer and workspace version differ' >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  digest() { value=$(sha256sum "$1") || return; printf '%s\n' "$value" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  digest() { value=$(shasum -a 256 "$1") || return; printf '%s\n' "$value" | awk '{print $1}'; }
else
  echo 'package-release: SHA256 utility is required' >&2; exit 1
fi
all='aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl'
[ "$#" -gt 0 ] || set -- $all
seen=' '
source_hash=$(sh "$root/scripts/source-fingerprint.sh" "$root")
stage=$(mktemp -d)
trap 'rm -rf "$stage"' 0
trap 'exit 129' 1
trap 'exit 130' 2
trap 'exit 143' 15
# Validate ALL selected payloads before changing output files or Formulae.
for target do
  case "$target" in aarch64-apple-darwin|x86_64-apple-darwin|aarch64-unknown-linux-musl|x86_64-unknown-linux-musl) ;; *) echo 'package-release: unsupported target' >&2; exit 1;; esac
  case "$seen" in *" $target "*) echo 'package-release: duplicate target' >&2; exit 1;; esac
  seen="$seen$target "
  proof="$root/bin/$target/build-info"
  [ -f "$proof" ] || { echo "package-release: fresh build proof missing for $target; run scripts/build.sh" >&2; exit 1; }
  [ "$(sed -n '1p' "$proof")" = "version=$version" ] && [ "$(sed -n '2p' "$proof")" = "source_sha256=$source_hash" ] \
    || { echo "package-release: stale build for $target; rebuild current source" >&2; exit 1; }
  [ "$(wc -l < "$proof" | tr -d ' ')" -eq 5 ] || { echo 'package-release: malformed build proof' >&2; exit 1; }
  for name in commitguard gh-commit-guard commitlint; do
    path="$root/bin/$target/$name"
    [ -f "$path" ] && [ -x "$path" ] || { echo "package-release: executable missing for $target/$name" >&2; exit 1; }
    hash=$(digest "$path")
    [ "$(grep -Fxc "$hash  $name" "$proof" || true)" -eq 1 ] || { echo "package-release: payload digest mismatch for $target/$name" >&2; exit 1; }
  done
  mkdir "$stage/$target"
  cp "$proof" "$stage/$target/build-info"
  for name in commitguard gh-commit-guard commitlint; do
    cp "$root/bin/$target/$name" "$stage/$target/$name"
    hash=$(digest "$stage/$target/$name")
    [ "$(grep -Fxc "$hash  $name" "$stage/$target/build-info" || true)" -eq 1 ] \
      || { echo 'package-release: payload changed while staging' >&2; exit 1; }
  done
done
cp "$root/LICENSE" "$root/README.md" "$root/install.sh" "$stage/"
[ "$(sh "$root/scripts/source-fingerprint.sh" "$root")" = "$source_hash" ] || { echo 'package-release: source changed while staging; rebuild' >&2; exit 1; }
if [ "$#" -eq 4 ]; then
  mkdir "$stage/Formula"
  dist="$root/dist/v$version"
else
  selection=$(printf '%s\n' "$@" | LC_ALL=C sort | tr '\n' '+')
  dist="$root/dist/partial/v$version/${selection%+}"
fi
mkdir -p "$dist"
: > "$dist/SHA256SUMS"
for target do
  cp "$stage/$target/gh-commit-guard" "$stage/$target/commitguard" "$stage/$target/commitlint" "$stage/"
  for kind in combined guard lint; do
    case "$kind" in
      combined) archive="commitlint-rust-$target.tar.gz"; members='gh-commit-guard commitlint LICENSE README.md';;
      guard) archive="commitguard-$target.tar.gz"; members='commitguard LICENSE README.md';;
      lint) archive="commitlint-only-$target.tar.gz"; members='commitlint LICENSE README.md';;
    esac
    COPYFILE_DISABLE=1 tar -czf "$dist/$archive" -C "$stage" $members
    hash=$(digest "$dist/$archive")
    printf '%s  %s\n' "$hash" "$archive" >> "$dist/SHA256SUMS"
  done
done
cp "$stage/install.sh" "$dist/install.sh"
hash=$(digest "$dist/install.sh")
printf '%s  install.sh\n' "$hash" >> "$dist/SHA256SUMS"
# Partial local builds are useful for smoke tests, but cannot produce universal Formulae.
if [ "$#" -eq 4 ]; then
  for kind in lint guard legacy; do
    case "$kind" in
      lint) formula="$stage/Formula/commitlint.rb"; class=Commitlint; desc='Native Conventional Commits message checker'; prefix=commitlint-only;;
      guard) formula="$stage/Formula/commitguard.rb"; class=Commitguard; desc='Native Git and gh identity and outgoing commit guard'; prefix=commitguard;;
      legacy) formula="$stage/Formula/commitlint-rust.rb"; class=CommitlintRust; desc='Compatible combined lint and Git identity guard distribution'; prefix=commitlint-rust;;
    esac
    cat > "$formula" <<HEADER
class $class < Formula
  desc "$desc"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "$version"
  license "MIT"
HEADER
    for os in macos linux; do
      printf '\n  on_%s do\n' "$os" >> "$formula"
      for cpu in arm intel; do
        case "$os:$cpu" in
          macos:arm) target=aarch64-apple-darwin;; macos:intel) target=x86_64-apple-darwin;;
          linux:arm) target=aarch64-unknown-linux-musl;; linux:intel) target=x86_64-unknown-linux-musl;;
        esac
        archive="$prefix-$target.tar.gz"
        hash=$(digest "$dist/$archive")
        printf '    on_%s do\n      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v%s/%s"\n      sha256 "%s"\n    end\n' "$cpu" "$version" "$archive" "$hash" >> "$formula"
      done
      printf '  end\n' >> "$formula"
    done
    if [ "$kind" = guard ]; then
      cat >> "$formula" <<'RUBY'

  depends_on "gh"
  depends_on "git"
  conflicts_with "commitlint-rust", because: "both provide the legacy guard command"

  def install
    bin.install "commitguard"
    bin.install_symlink "commitguard" => "gh-commit-guard"
  end

  def caveats
    <<~EOS
      Activate Git guards and Codex/Claude skills explicitly with:
        commitguard install
      Finish active history repairs before replacing their installed guard.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/commitguard version")
    assert_match version.to_s, shell_output("#{bin}/gh-commit-guard version")
  end
end
RUBY
    elif [ "$kind" = legacy ]; then
      cat >> "$formula" <<'RUBY'

  depends_on "gh"
  depends_on "git"
  conflicts_with "commitguard", because: "both provide the legacy guard command"

  def install
    bin.install "gh-commit-guard"
    bin.install_symlink "gh-commit-guard" => "commitguard"
    bin.install "commitlint" => "commitlint-rust"
  end

  def caveats
    <<~EOS
      This combined compatibility package preserves commands from v0.1.0.
      New installations can choose the independent commitlint and commitguard formulae.
      Guard activation stays explicit: commitguard install
      Finish active history repairs before replacing their installed guard.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gh-commit-guard version")
    assert_match version.to_s, shell_output("#{bin}/commitguard version")
    assert_match version.to_s, shell_output("#{bin}/commitlint-rust --version")
    assert_match "Usage:", shell_output("#{bin}/commitlint-rust --help")
  end
end
RUBY
    else
      cat >> "$formula" <<'RUBY'

  def install
    bin.install "commitlint"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/commitlint --version")
    assert_match "Usage:", shell_output("#{bin}/commitlint --help")
    assert_equal "", pipe_output("#{bin}/commitlint", "fix: validate standalone messages\n")
  end
end
RUBY
    fi
  done
  mkdir -p "$root/Formula"
  mv "$stage/Formula/"*.rb "$root/Formula/"
fi
printf 'Release assets ready for %s target(s): %s\n' "$#" "$dist"
