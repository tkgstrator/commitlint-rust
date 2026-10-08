#!/bin/sh
# Dev Container Feature installer (root, build time). Installs the pinned public
# Commitguard release binary and the static Git shim. No gh auth or token is used.
set -eu
umask 022

version=v0.2.0
release=https://github.com/tkgstrator/commitlint-rust/releases/download/$version
base=/usr/local/share/commitguard

die() { echo "commitguard feature: $*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "must run as root"

# Debian/Ubuntu only.
distro=$( . /etc/os-release 2>/dev/null && echo "${ID:-} ${ID_LIKE:-}" ) || die "cannot read /etc/os-release"
case " $distro " in
  *" debian "*|*" ubuntu "*) ;;
  *) die "only Debian/Ubuntu images are supported" ;;
esac

case "$(uname -m)" in
  x86_64|amd64)
    target=x86_64-unknown-linux-musl
    digest=73cada7f1e7912e1140c1f6aef620e7d592ff24fad8379de8875bdc77ad9e52b ;;
  aarch64|arm64)
    target=aarch64-unknown-linux-musl
    digest=a1edaaa46c25dfcd47fb0b6c38d6f58047276fc112e079587236a5ca706cca93 ;;
  *) die "only amd64/arm64 are supported" ;;
esac

case "${AUTOACTIVATE:-true}" in
  true|false) auto=${AUTOACTIVATE:-true} ;;
  *) die "autoActivate must be true or false" ;;
esac

here=$(cd "$(dirname "$0")" && pwd)
[ -f "$here/setup.sh" ] && [ -f "$here/git-shim.sh" ] || die "feature files missing"

if ! command -v curl >/dev/null 2>&1; then
  command -v apt-get >/dev/null 2>&1 || die "curl is required"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -y
  apt-get install -y --no-install-recommends curl ca-certificates
  rm -rf /var/lib/apt/lists/*
fi
for tool in tar gzip sha256sum readlink install mktemp; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done

# The real Git must already exist (dependsOn git); never record our own shim.
bootstrap=
oldifs=$IFS; IFS=:
for dir in $PATH; do
  [ "$dir" = "$base/bin" ] || bootstrap=${bootstrap:+$bootstrap:}$dir
done
IFS=$oldifs
native=$(PATH=$bootstrap command -v git) || die "git is required (dependsOn git feature)"
native=$(readlink -f "$native") || die "cannot resolve git"
[ -x "$native" ] && [ -f "$native" ] || die "git is not an executable file"
case "$native" in "$base"/*) die "git resolves to the guard shim" ;; esac
PATH=$bootstrap command -v gh >/dev/null 2>&1 || die "gh is required (dependsOn github-cli feature)"

tmp=$(mktemp -d) || die "cannot create temporary directory"
trap 'rm -rf "$tmp"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

archive=commitguard-$target.tar.gz
curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 --retry 3 \
  --output "$tmp/$archive" "$release/$archive" || die "download failed: $release/$archive"
actual=$(sha256sum "$tmp/$archive") || die "cannot hash archive"
[ "${actual%% *}" = "$digest" ] || die "checksum mismatch for $archive"

expected="LICENSE
README.md
commitguard"
names=$(tar -tzf "$tmp/$archive") || die "cannot list archive"
[ "$(printf '%s\n' "$names" | LC_ALL=C sort)" = "$expected" ] \
  || die "archive must contain exactly: commitguard LICENSE README.md"
types=$(tar -tvzf "$tmp/$archive") || die "cannot list archive"
[ "$(printf '%s\n' "$types" | wc -l | tr -d ' ')" = 3 ] || die "unexpected archive listing"
printf '%s\n' "$types" | while IFS= read -r line; do
  case "$line" in -*) ;; *) exit 1 ;; esac
done || die "archive entries must be regular files"

mkdir "$tmp/payload"
tar -xzf "$tmp/$archive" -C "$tmp/payload" || die "extraction failed"
for f in LICENSE README.md commitguard; do
  [ -f "$tmp/payload/$f" ] && [ ! -L "$tmp/payload/$f" ] || die "unexpected extracted entry: $f"
done
[ "$(find "$tmp/payload" -mindepth 1 | wc -l | tr -d ' ')" = 3 ] || die "unexpected extracted files"

mkdir -p /usr/local/bin
install -m 0755 "$tmp/payload/commitguard" /usr/local/bin/commitguard
ln -sfn commitguard /usr/local/bin/gh-commit-guard

# Root-owned persisted state. The shim is installed last so a failed install
# never leaves a blocking Git behind.
mkdir -p "$base/bin"
chmod 0755 "$base" "$base/bin"
printf '%s\n' "$native" > "$base/native-git.tmp"
chmod 0644 "$base/native-git.tmp"; mv -f "$base/native-git.tmp" "$base/native-git"
printf 'auto=%s\nversion=%s\n' "$auto" "$version" > "$base/options.tmp"
chmod 0644 "$base/options.tmp"; mv -f "$base/options.tmp" "$base/options"
install -m 0755 "$here/setup.sh" "$base/setup"
ln -sfn ../share/commitguard/setup /usr/local/bin/commitguard-devcontainer-setup
mkdir -p /etc/profile.d
printf '%s\n' 'case ":$PATH:" in *:/usr/local/share/commitguard/bin:*) ;; *) PATH="/usr/local/share/commitguard/bin:$PATH"; export PATH ;; esac' > /etc/profile.d/commitguard-feature.sh
chmod 0644 /etc/profile.d/commitguard-feature.sh
install -m 0755 "$here/git-shim.sh" "$base/bin/git.tmp"
mv -f "$base/bin/git.tmp" "$base/bin/git"
echo "commitguard feature: installed $version ($target), autoActivate=$auto"
