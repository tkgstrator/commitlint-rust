#!/bin/sh
# Commitguard Dev Container activation helper. Runs as the real remoteUser.
#   setup --auto   container start (honours the root-owned autoActivate option)
#   setup          manual activation (always attempted)
# The ready marker is written only after install, config/version and guarded
# Git checks succeed; any earlier failure leaves the Git shim blocked.
set -eu
umask 077

base=/usr/local/share/commitguard
cg=/usr/local/bin/commitguard
version=v0.2.0

die() {
  echo "commitguard: $*" >&2
  echo "commitguard: run 'gh auth login -h github.com' with a human account, open the workspace repository, then retry '$base/setup'." >&2
  exit 1
}

mode=manual
case "${1:-}" in
  "") ;;
  --auto) mode=auto ;;
  *) echo "usage: setup [--auto]" >&2; exit 2 ;;
esac

auto=true
if [ -r "$base/options" ]; then
  auto=
  while IFS='=' read -r key value; do
    [ "$key" = auto ] && auto=$value
  done < "$base/options"
fi
if [ "$mode" = auto ] && [ "$auto" = false ]; then
  echo "commitguard: autoActivate is disabled; run '$base/setup' to activate." >&2
  exit 0
fi

[ -x "$cg" ] && [ -r "$base/native-git" ] || die "feature is not installed"
IFS= read -r native < "$base/native-git" || native=
[ -n "$native" ] && [ -x "$native" ] || die "persisted native Git is unavailable"

# Bootstrap PATH: remove only the exact static shim directory.
bootstrap=
oldifs=$IFS; IFS=:
for dir in $PATH; do
  [ "$dir" = "$base/bin" ] || bootstrap=${bootstrap:+$bootstrap:}$dir
done
IFS=$oldifs
PATH=$bootstrap
export PATH
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

case "${HOME:-}" in
  /*) ;;
  *) die "HOME must be an absolute container-private directory" ;;
esac
[ "$HOME" != / ] && [ -d "$HOME" ] && [ -O "$HOME" ] || die "HOME must be an owned directory"
marker_dir=$HOME/.local/share/commitguard
marker=$marker_dir/feature-ready
# A ready marker records the last successful activation. Validate every target
# before touching it; an existing valid native guard continues enforcing policy.

# Reject a path (and every ancestor) that is a symlink.
nosymlink() {
  p=$1
  while [ -n "$p" ] && [ "$p" != / ]; do
    [ ! -L "$p" ] || die "symlink in managed path: $p"
    p=${p%/*}
  done
}

guard_parent=$HOME/.local/share/gh-commit-identity
guard=$guard_parent/guard
if [ -n "${GIT_CONFIG_GLOBAL:-}" ] && [ "$GIT_CONFIG_GLOBAL" != "$HOME/.gitconfig" ]; then
  die "external GIT_CONFIG_GLOBAL is not supported"
fi

managed="$HOME
$guard_parent
$guard
$marker_dir
$HOME/.gitconfig"
for f in .zshenv .zprofile .zshrc .profile .bashrc .bash_profile .bash_login; do
  managed="$managed
$HOME/$f"
done
oldifs=$IFS
IFS='
'
set -f
for p in $managed; do
  nosymlink "$p"
done
set +f
IFS=$oldifs

# Reject nested redirects as well as a redirected managed root. The published
# installer can write skill/bin descendants, so checking just .codex is insufficient.
private_tree() {
  nosymlink "$1"
  if [ -d "$1" ]; then
    links=$(find "$1" -type l -print -quit) || die "cannot inspect managed tree: $1"
    [ -z "$links" ] || die "symlink in managed tree: $links"
  fi
}
for tree in "$guard_parent" "$marker_dir"; do
  private_tree "$tree"
done

# Workspace: the repository containing the start directory.
ws=$("$native" rev-parse --show-toplevel 2>/dev/null) || die "no Git repository in $(pwd)"
ws=$(cd -P "$ws" 2>/dev/null && pwd -P) || die "cannot resolve workspace"
for sub in .codex .claude; do
  private_tree "$ws/$sub"
done
for file in AGENTS.md CLAUDE.md; do
  nosymlink "$ws/$file"
done

# Mount safety. Root and separate host mounts at HOME/.codex or HOME/.claude are
# allowed (never written). Reject any mount at or above a managed HOME path,
# mounts nested in guard state or managed workspace agent paths. The workspace
# base mount and unrelated volumes such as node_modules remain allowed.
[ -r /proc/self/mountinfo ] || die "cannot inspect mounts"
set -f
while IFS= read -r line; do
  set -- $line
  [ $# -ge 5 ] || continue
  mp=$(printf '%s' "$5" | sed 's/\\040/ /g; s/\\011/	/g; s/\\012/\n/g; s/\\134/\\/g')
  [ "$mp" = / ] && continue
  oldifs=$IFS; IFS='
'
  for p in $managed; do
    case "$p/" in
      "$mp"/*) die "managed path $p is on a mount ($mp); a container-private HOME is required" ;;
    esac
  done
  IFS=$oldifs
  case "$mp/" in
    "$guard_parent"/*|"$marker_dir"/*) die "mount nested in guard state: $mp" ;;
    "$ws/.codex"/*|"$ws/.claude"/*|"$ws/AGENTS.md/"|"$ws/CLAUDE.md/") die "mount at a managed workspace agent path: $mp" ;;
  esac
done < /proc/self/mountinfo
set +f

# v0.2.0 installs enforcement before login. Every actual commit/push still
# authenticates through the native guard; no credentials are needed for setup.
rm -f "$marker"
[ ! -e "$marker" ] || die "cannot clear previous activation marker"

mkdir -p "$marker_dir"
CODEX_HOME=$ws/.codex CLAUDE_CONFIG_DIR=$ws/.claude "$cg" install --container --repo "$ws" \
  || die "native install failed"

[ -f "$guard/config.json" ] && [ ! -L "$guard/config.json" ] || die "installed configuration missing"
grep -Fq "\"git\": \"$native\"" "$guard/config.json" || die "installed configuration does not match native Git"
[ -x "$guard/bin/commitguard" ] && [ -x "$guard/bin/git" ] || die "installed guard executables missing"
reported=$("$guard/bin/commitguard" --config "$guard/config.json" version) || die "installed version check failed"
[ "$reported" = "commitguard ${version#v}" ] || die "unexpected installed version: $reported"
inside=$("$guard/bin/git" -C "$ws" rev-parse --is-inside-work-tree) || die "guarded Git check failed"
[ "$inside" = true ] || die "guarded Git check failed"

printf 'version=%s\n' "$version" > "$marker.tmp"
mv -f "$marker.tmp" "$marker"
echo "commitguard: guarded Git is active for $ws"
