#!/bin/sh
# Isolated Linux tests of the actual pinned public binary and Feature helpers.
set -eu
feature=/usr/local/share/commitguard
fixture=$(mktemp -d /tmp/commitguard-feature-tests.XXXXXX)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/tools" "$fixture/workspace"
cat > "$fixture/tools/gh" <<'GH'
#!/bin/sh
case "$*" in
  --version) echo 'gh test fixture';;
  'api --hostname github.com user')
    [ "${TEST_AUTH:-user}" != missing ] || exit 1
    case "${TEST_AUTH:-user}" in
      bot) echo '{"login":"tester","id":44,"type":"Bot"}';;
      *) echo '{"login":"tester","id":44,"type":"User"}';;
    esac;;
  *) exit 99;;
esac
GH
chmod +x "$fixture/tools/gh"
PATH="$fixture/tools:$feature/bin:$PATH"
export PATH
GIT_AUTHOR_NAME=tester GIT_AUTHOR_EMAIL=44+tester@users.noreply.github.com GIT_COMMITTER_NAME=tester GIT_COMMITTER_EMAIL=44+tester@users.noreply.github.com
export GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL
unset GIT_CONFIG_GLOBAL GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS XDG_STATE_HOME CODEX_HOME CLAUDE_CONFIG_DIR GH_TOKEN GITHUB_TOKEN GH_DEBUG DEBUG
native=$(cat "$feature/native-git")
"$native" -C "$fixture/workspace" init -q --initial-branch=main
cd "$fixture/workspace"
new_home() {
  HOME="$fixture/$1"
  export HOME
  mkdir -p "$HOME"
}
refuse() {
  if timeout 25 "$@" > "$fixture/error" 2>&1; then
    echo "unexpected success: $*" >&2; exit 1
  else
    status=$?
    [ "$status" != 124 ] || { cat "$fixture/error"; echo 'unexpected hang' >&2; exit 1; }
  fi
}
new_home unauthenticated
TEST_AUTH=missing; export TEST_AUTH
refuse git status
timeout 25 "$feature/setup" --auto
git status --porcelain > /dev/null
printf 'tracked\n' > tracked
git add tracked
refuse git commit -m 'feat: fixture'
grep -q 'gh-identity.*refused' "$fixture/error"
echo 'PASS unauthenticated setup enables enforcement; commits remain blocked'
TEST_AUTH=bot
refuse git commit -m 'feat: fixture'
grep -q 'gh-identity.*refused' "$fixture/error"
echo 'PASS bot authentication cannot commit'
TEST_AUTH=user
new_home authenticated
mkdir -p "$fixture/host-codex" "$fixture/host-claude"
printf 'host sentinel\n' > "$fixture/host-codex/sentinel"
printf 'host sentinel\n' > "$fixture/host-claude/sentinel"
ln -s "$fixture/host-codex" "$HOME/.codex"
ln -s "$fixture/host-claude" "$HOME/.claude"
timeout 25 "$feature/setup" --auto
[ "$(cat "$fixture/host-codex/sentinel")" = 'host sentinel' ]
[ "$(cat "$fixture/host-claude/sentinel")" = 'host sentinel' ]
git status --porcelain > /dev/null
timeout 25 "$feature/setup" --auto
[ "$(grep -c '<!-- gh-commit-identity -->' AGENTS.md)" = 1 ]
echo 'PASS authenticated nonlogin activation and repeated startup preserve host agent homes'
printf 'tracked\n' > tracked
git add tracked
refuse env GIT_AUTHOR_NAME=wrong GIT_AUTHOR_EMAIL=wrong@example.test GIT_COMMITTER_NAME=wrong GIT_COMMITTER_EMAIL=wrong@example.test git commit --no-verify -m 'feat: fixture'
refuse git -c core.hooksPath=/dev/null commit -m 'invalid message'
GIT_AUTHOR_NAME=tester GIT_AUTHOR_EMAIL=44+tester@users.noreply.github.com GIT_COMMITTER_NAME=tester GIT_COMMITTER_EMAIL=44+tester@users.noreply.github.com git -c commit.gpgsign=false commit --no-gpg-sign -m 'feat: verified fixture'
"$HOME/.local/share/gh-commit-identity/guard/bin/commitguard" commits HEAD
echo 'PASS actual guard rejects ordinary bypass flags and accepts verified identity'
"$native" init -q --bare "$fixture/remote.git"
git remote add origin "$fixture/remote.git"
TEST_AUTH=missing
refuse git push origin HEAD:main
grep -q 'gh-identity.*refused' "$fixture/error"
TEST_AUTH=bot
refuse git push origin HEAD:main
grep -q 'gh-identity.*refused' "$fixture/error"
TEST_AUTH=user
git push origin HEAD:main
echo 'PASS actual push authentication rejects missing/bot accounts'
/bin/bash -lc 'command -v git' | grep -Eq 'commitguard/bin/git|gh-commit-identity/guard/bin/git'
echo 'PASS login and nonlogin shells resolve guarded Git'
new_home 'home with spaces'
timeout 25 "$feature/setup" --auto
git status --porcelain > /dev/null
echo 'PASS HOME paths with spaces'
new_home unsafe
mkdir -p "$fixture/host-state/share/commitguard"
printf 'retain\n' > "$fixture/host-state/share/commitguard/feature-ready"
ln -s "$fixture/host-state" "$HOME/.local"
refuse "$feature/setup" --auto
[ "$(cat "$fixture/host-state/share/commitguard/feature-ready")" = retain ]
echo 'PASS unsafe home symlinks are rejected before mutation'
new_home external-config
GIT_CONFIG_GLOBAL="$fixture/host-gitconfig"; export GIT_CONFIG_GLOBAL
printf '[alias]\n retained = status\n' > "$GIT_CONFIG_GLOBAL"
refuse "$feature/setup" --auto
grep -q retained "$GIT_CONFIG_GLOBAL"
unset GIT_CONFIG_GLOBAL
echo 'PASS external Git configuration remains unchanged'

new_home nested-links
mkdir -p "$fixture/external-skills"
printf 'retain\n' > "$fixture/external-skills/sentinel"
mv .codex/skills .codex/skills.original
ln -s "$fixture/external-skills" .codex/skills
refuse "$feature/setup" --auto
[ "$(cat "$fixture/external-skills/sentinel")" = retain ]
[ ! -e "$fixture/external-skills/gh-commit-identity" ]
rm .codex/skills
mv .codex/skills.original .codex/skills
echo 'PASS nested workspace agent symlinks cannot redirect installation'
