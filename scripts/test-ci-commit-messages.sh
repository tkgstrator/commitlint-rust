#!/usr/bin/env bash
# Route-selection regression test for the "Lint commit messages" workflow step.
# Extracts the step's shell, runs it against a mock git and a mock linter, and
# creates no Git objects. Usage: bash scripts/test-ci-commit-messages.sh [workflow.yaml]
set -eu

wf=${1:-.github/workflows/integration.yaml}
[ -f "$wf" ] || { echo "workflow not found: $wf" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

# Extract the run block of the step (pure bash; no awk/python needed).
state=0
: > "$tmp/step.sh"
while IFS= read -r line; do
  case $state in
    0) [ "$line" = "      - name: Lint commit messages" ] && state=1 ;;
    1) if [ "$line" = "        run: |" ]; then state=2
       else case $line in "      - "*) break ;; esac; fi ;;
    2) case $line in
         "          "*) printf "%s\\n" "${line#          }" >> "$tmp/step.sh" ;;
         "") printf "\\n" >> "$tmp/step.sh" ;;
         *) break ;;
       esac ;;
  esac
done < "$wf"
[ -s "$tmp/step.sh" ] || { echo 'could not extract the lint step' >&2; exit 1; }
bash -n "$tmp/step.sh"

mkdir -p "$tmp/bin" "$tmp/work/target/debug"
cat > "$tmp/bin/git" <<'MOCK'
#!/bin/sh
echo "git $*" >> "$MOCK_LOG"
case "$*" in
  "cat-file -e $MOCK_EXPECT_FROM^{commit}") [ "$MOCK_BASE_PRESENT" = 1 ] ;;
  "merge-base origin/master HEAD") [ -n "${MOCK_MERGE_BASE:-}" ] && echo "$MOCK_MERGE_BASE" ;;
  "rev-list --count $MOCK_EXPECT_FROM..$MOCK_EXPECT_TO") echo "$MOCK_COUNT" ;;
  *) echo "unexpected git call: $*" >> "$MOCK_LOG"; exit 99 ;;
esac
MOCK
cat > "$tmp/work/target/debug/commitlint" <<'MOCK'
#!/bin/sh
echo "lint $*" >> "$MOCK_LOG"
[ "$*" = "--from $MOCK_EXPECT_FROM --to $MOCK_EXPECT_TO" ] \
  || { echo "unexpected lint range: $*" >> "$MOCK_LOG"; exit 99; }
exit "${MOCK_LINT_RC:-0}"
MOCK
chmod +x "$tmp/bin/git" "$tmp/work/target/debug/commitlint"

fail=0
ZERO=0000000000000000000000000000000000000000
# run NAME EXPECT_STATUS EXPECT_LINT(yes|no) [VAR=value ...]
run() {
  name=$1 want_rc=$2 want_lint=$3
  shift 3
  : > "$tmp/log"
  set +e
  (
    cd "$tmp/work"
    env -i PATH="$tmp/bin:$PATH" MOCK_LOG="$tmp/log" MOCK_BASE_PRESENT=1 MOCK_COUNT=0 \
      MOCK_EXPECT_FROM="$A" MOCK_EXPECT_TO="$B" \
      EVENT_NAME='' REF_TYPE=branch PR_BASE_SHA='' PR_HEAD_SHA='' PUSH_BEFORE_SHA='' \
      PUSH_SHA='' BASE_BRANCH=master "$@" bash "$tmp/step.sh"
  ) > "$tmp/out" 2>&1
  rc=$?
  set -e
  got_lint=no
  grep -q '^lint ' "$tmp/log" && got_lint=yes
  zero_ok=0
  [ "$want_rc" = 0 ] && [ "$rc" = 0 ] && zero_ok=1
  [ "$want_rc" = nz ] && [ "$rc" != 0 ] && zero_ok=1
  if [ "$zero_ok" = 1 ] && [ "$got_lint" = "$want_lint" ] && ! grep -q '^unexpected ' "$tmp/log"; then
    echo "ok   $name"
  else
    echo "FAIL $name (rc=$rc want=$want_rc, lint=$got_lint want=$want_lint)"
    sed 's/^/     /' "$tmp/out" "$tmp/log"
    fail=1
  fi
}

A=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
B=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb

run 'tag push inherited merge: no lint' 0 no EVENT_NAME=push REF_TYPE=tag PUSH_BEFORE_SHA=$ZERO PUSH_SHA=$B
run 'tag workflow_dispatch: no lint' 0 no EVENT_NAME=workflow_dispatch REF_TYPE=tag MOCK_MERGE_BASE=$A MOCK_COUNT=3
run 'unknown event fails closed' nz no EVENT_NAME=schedule
run 'empty event fails closed' nz no
run 'PR good range lints' 0 yes EVENT_NAME=pull_request PR_BASE_SHA=$A PR_HEAD_SHA=$B MOCK_COUNT=2
run 'PR bad range refuses' nz yes EVENT_NAME=pull_request PR_BASE_SHA=$A PR_HEAD_SHA=$B MOCK_COUNT=2 MOCK_LINT_RC=1
run 'PR missing head refuses' nz no EVENT_NAME=pull_request PR_BASE_SHA=$A
run 'push bad range refuses' nz yes EVENT_NAME=push PUSH_BEFORE_SHA=$A PUSH_SHA=$B MOCK_COUNT=1 MOCK_LINT_RC=1
run 'push good range lints' 0 yes EVENT_NAME=push PUSH_BEFORE_SHA=$A PUSH_SHA=$B MOCK_COUNT=1
run 'push missing base object refuses' nz no EVENT_NAME=push PUSH_BEFORE_SHA=$A PUSH_SHA=$B MOCK_BASE_PRESENT=0
run 'push missing base value refuses' nz no EVENT_NAME=push PUSH_SHA=$B
run 'first push bad range refuses' nz yes EVENT_NAME=push PUSH_BEFORE_SHA=$ZERO PUSH_SHA=$B MOCK_MERGE_BASE=$A MOCK_COUNT=2 MOCK_LINT_RC=1
run 'first push unresolved base refuses' nz no EVENT_NAME=push PUSH_BEFORE_SHA=$ZERO PUSH_SHA=$B
run 'manual on master empty range: no lint' 0 no EVENT_NAME=workflow_dispatch MOCK_MERGE_BASE=$B MOCK_EXPECT_FROM=$B MOCK_EXPECT_TO=HEAD MOCK_COUNT=0
run 'manual branch bad range refuses' nz yes EVENT_NAME=workflow_dispatch MOCK_MERGE_BASE=$A MOCK_EXPECT_TO=HEAD MOCK_COUNT=2 MOCK_LINT_RC=1
run 'manual branch good range lints' 0 yes EVENT_NAME=workflow_dispatch MOCK_MERGE_BASE=$A MOCK_EXPECT_TO=HEAD MOCK_COUNT=2
run 'manual unresolved base refuses' nz no EVENT_NAME=workflow_dispatch

[ "$fail" = 0 ] && echo 'all CI commit-message routing tests passed'
exit "$fail"
