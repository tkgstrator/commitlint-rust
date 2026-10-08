#!/bin/sh
# Static Git shim. Auto mode (root-owned option) fails closed until the helper
# has activated the guard; otherwise it runs persisted native Git or guarded Git.
base=/usr/local/share/commitguard

auto=true
if [ -r "$base/options" ]; then
  auto=
  while IFS='=' read -r key value; do
    [ "$key" = auto ] && auto=$value
  done < "$base/options"
fi
case "$auto" in true|false) ;; *) auto=true ;; esac

blocked() {
  echo "commitguard: Git is blocked: $1" >&2
  echo "commitguard: run 'gh auth login -h github.com' as a human account, then '$base/setup' from the workspace and retry." >&2
  exit 1
}

ready=0
guard=
marker=${HOME:+$HOME/.local/share/commitguard/feature-ready}
if [ -n "$marker" ] && { [ -e "$marker" ] || [ -L "$marker" ]; }; then
  case "$HOME" in /*) ;; *) blocked "HOME is not absolute" ;; esac
  guard=$HOME/.local/share/gh-commit-identity/guard
  line=
  if [ -f "$marker" ] && [ ! -L "$marker" ] && [ -O "$marker" ]; then
    IFS= read -r line < "$marker" || line=
  fi
  [ "$line" = "version=v0.2.0" ] \
    && [ -f "$guard/config.json" ] && [ ! -L "$guard/config.json" ] \
    && [ -f "$guard/bin/git" ] && [ -x "$guard/bin/git" ] && [ ! -L "$guard/bin/git" ] \
    && [ -x "$guard/bin/commitguard" ] \
    || blocked "guard activation is missing or invalid"
  ready=1
fi

if [ "$ready" = 1 ]; then
  exec "$guard/bin/git" "$@"
fi
[ "$auto" = true ] && blocked "guard is not activated"
native=
IFS= read -r native < "$base/native-git" 2>/dev/null || native=
[ -n "$native" ] && [ -x "$native" ] || { echo "commitguard: persisted native Git is unavailable" >&2; exit 1; }
exec "$native" "$@"
