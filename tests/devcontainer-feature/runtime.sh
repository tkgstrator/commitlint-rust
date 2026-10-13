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
  'auth token --hostname github.com')
    [ "${TEST_AUTH:-user}" != missing ] || exit 1
    printf '%s\n' "${TEST_GH_TOKEN:-fixture-token-${TEST_AUTH:-user}}";;
  'config get user --host github.com') printf '%s\n' "${TEST_SELECTED_USER:-tester}";;
  'api --hostname github.com user')
    printf 'api\n' >> "$TEST_API_LOG"
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
TEST_API_LOG=$fixture/api-calls; export TEST_API_LOG
: > "$TEST_API_LOG"
GIT_AUTHOR_NAME=tester GIT_AUTHOR_EMAIL=44+tester@users.noreply.github.com GIT_COMMITTER_NAME=tester GIT_COMMITTER_EMAIL=44+tester@users.noreply.github.com
export GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL
unset GIT_CONFIG_GLOBAL GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS XDG_STATE_HOME XDG_CONFIG_HOME CODEX_HOME CLAUDE_CONFIG_DIR GH_CONFIG_DIR GH_TOKEN GITHUB_TOKEN GH_HOST GH_ENTERPRISE_TOKEN GITHUB_ENTERPRISE_TOKEN GH_DEBUG DEBUG COMMITGUARD_STRICT TEST_GH_TOKEN TEST_SELECTED_USER
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
# Standalone lint works before gh login and guard activation.
[ "$(commitlint-rust --version)" = 'commitlint-rust 0.3.0' ]
printf 'feat: standalone fixture\n' | commitlint-rust
if printf 'invalid message\n' | commitlint-rust > "$fixture/lint-error" 2>&1; then
  echo 'standalone lint accepted invalid message' >&2; exit 1
fi
grep -q 'commitlint: invalid Conventional Commit header' "$fixture/lint-error"
echo 'PASS standalone lint validates stdin without authentication'
# Configuration discovery/explicit selection remain independent of authentication.
printf '%s\n' 'rules:' '  header-max-length: [2, always, 12]' > .commitlintrc.yaml
printf 'fix: small\n' | commitlint-rust
if printf 'feat: standalone fixture\n' | commitlint-rust > "$fixture/lint-error" 2>&1; then
  echo 'standalone lint ignored YAML configuration' >&2; exit 1
fi
grep -q header-max-length "$fixture/lint-error"
printf '%s\n' '{"rules":{"type-enum":[2,"always",["fix"]]}}' > "$fixture/lint.json"
printf 'fix: explicit configuration\n' | commitlint-rust --config "$fixture/lint.json"
rm .commitlintrc.yaml
echo 'PASS standalone JSON/YAML configuration needs no authentication'
refuse git status
refuse "$feature/setup" --auto
grep -q 'native install failed' "$fixture/error"
[ ! -e "$HOME/.local/share/commitguard/feature-ready" ]
refuse git status
refuse git commit -m 'feat: fixture'
grep -q 'Git is blocked' "$fixture/error"
echo 'PASS unauthenticated activation has no ready marker and leaves Git blocked'
TEST_AUTH=bot
refuse "$feature/setup" --auto
grep -q 'native install failed' "$fixture/error"
[ ! -e "$HOME/.local/share/commitguard/feature-ready" ]
refuse git status
echo 'PASS bot authentication cannot activate the guard'
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
[ "$(cat "$HOME/.local/share/commitguard/feature-ready")" = 'version=v0.3.0' ]
api_before=$(wc -l < "$TEST_API_LOG")
printf 'tracked\n' > tracked
git add tracked
refuse env GIT_AUTHOR_NAME=wrong GIT_AUTHOR_EMAIL=wrong@example.test GIT_COMMITTER_NAME=wrong GIT_COMMITTER_EMAIL=wrong@example.test git commit --no-verify -m 'feat: fixture'
refuse git -c core.hooksPath=/dev/null commit -m 'invalid message'
GIT_AUTHOR_NAME=tester GIT_AUTHOR_EMAIL=44+tester@users.noreply.github.com GIT_COMMITTER_NAME=tester GIT_COMMITTER_EMAIL=44+tester@users.noreply.github.com git -c commit.gpgsign=false commit --no-gpg-sign -m 'feat: verified fixture'
"$HOME/.local/share/gh-commit-identity/guard/bin/commitguard" commits HEAD
echo 'PASS actual guard rejects ordinary bypass flags and accepts verified identity'
# A 128-character subject plus valid final attribution passes only the guard.
awk 'BEGIN { printf "feat: "; for (i=0; i<122; i++) printf "x" }' > "$fixture/coauthor-message"
[ "$(wc -c < "$fixture/coauthor-message" | tr -d ' ')" = 128 ]
printf '\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n' >> "$fixture/coauthor-message"
if commitlint-rust --edit "$fixture/coauthor-message" > "$fixture/lint-error" 2>&1; then
  echo 'standalone lint unexpectedly excluded coauthor length' >&2; exit 1
fi
# The repository's standalone linter configuration does not weaken guard checks.
printf '%s\n' '{"rules":{"header-max-length":[2,"always",5]}}' > .commitlintrc.json
git -c commit.gpgsign=false commit --no-gpg-sign --allow-empty -F "$fixture/coauthor-message"
"$HOME/.local/share/gh-commit-identity/guard/bin/commitguard" commits HEAD
rm .commitlintrc.json
# Removing a valid credit cannot make 129 counted characters acceptable.
head_before=$("$native" rev-parse HEAD)
awk 'BEGIN { printf "feat: "; for (i=0; i<123; i++) printf "x" }' > "$fixture/overlong-coauthor-message"
[ "$(wc -c < "$fixture/overlong-coauthor-message" | tr -d ' ')" = 129 ]
printf '\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n' >> "$fixture/overlong-coauthor-message"
refuse git -c commit.gpgsign=false commit --no-gpg-sign --allow-empty -F "$fixture/overlong-coauthor-message"
grep -q 'entire message exceeds 128 characters' "$fixture/error"
[ "$("$native" rev-parse HEAD)" = "$head_before" ]
[ "$(wc -l < "$TEST_API_LOG")" = "$api_before" ]
echo 'PASS cached commits avoid API calls and coauthor exemption retains the 128-character bound'

# Each independent context change misses the existing cache without API access.
# The mock API keeps returning the verified human tester, matching the native
# auth fixture: selected-user text binds the cache independently of API identity.
context_change() {
  context=$1
  head_before=$("$native" rev-parse HEAD)
  api_log_before=$(cat "$TEST_API_LOG")
  refuse git -c commit.gpgsign=false commit --no-gpg-sign --allow-empty -m 'test: changed cached context'
  grep -q 'identity cache missing or changed' "$fixture/error"
  [ "$("$native" rev-parse HEAD)" = "$head_before" ]
  [ "$(cat "$TEST_API_LOG")" = "$api_log_before" ]
  api_count_before=$(wc -l < "$TEST_API_LOG" | tr -d ' ')
  timeout 25 "$HOME/.local/share/gh-commit-identity/guard/bin/commitguard" --strict account
  [ "$(wc -l < "$TEST_API_LOG" | tr -d ' ')" = "$((api_count_before + 1))" ]
  [ "$("$native" rev-parse HEAD)" = "$head_before" ]
  api_log_refreshed=$(cat "$TEST_API_LOG")
  git -c commit.gpgsign=false commit --no-gpg-sign --allow-empty -m 'test: recover cached context'
  [ "$("$native" rev-parse HEAD)" != "$head_before" ]
  "$HOME/.local/share/gh-commit-identity/guard/bin/commitguard" commits HEAD
  [ "$(cat "$TEST_API_LOG")" = "$api_log_refreshed" ]
  echo "PASS $context change refuses cached commits without API; strict refresh recovers"
}
TEST_GH_TOKEN=fixture-refreshed-token; export TEST_GH_TOKEN
context_change token
unset TEST_GH_TOKEN
TEST_SELECTED_USER=changed-selection; export TEST_SELECTED_USER
context_change selected-account
unset TEST_SELECTED_USER
GH_CONFIG_DIR=$fixture/alternate-gh-config; export GH_CONFIG_DIR
mkdir -p "$GH_CONFIG_DIR"
context_change gh-config-directory
unset GH_CONFIG_DIR

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
