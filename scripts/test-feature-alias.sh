#!/bin/sh
# Mocked-oras tests for publish-feature-alias.sh. State lives in $tmp/reg:
#   dst/<tag> = digest, src/<tag> = digest, man/<digest> = manifest JSON, writes = log.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/feature-alias-tests.XXXXXX")
trap 'rm -rf "$tmp"' 0
command -v jq >/dev/null || { echo 'test-feature-alias: jq required' >&2; exit 2; }
mkdir -p "$tmp/bin"
cat > "$tmp/bin/oras" <<'MOCK'
#!/bin/sh
# Fixture oras: FAIL_ON=<subcommand> fails it; FAIL_COPY_TAG=<tag> fails one copy.
R=$REG; cmd=$1; shift
case " $* " in *" --registry-config "*) ;; *) echo "missing --registry-config" >&2; exit 9;; esac
[ "${FAIL_ON:-}" != "$cmd" ] || { echo "network error" >&2; exit 1; }
args=; while [ $# -gt 0 ]; do case $1 in --registry-config) shift 2;; *) args="$args $1"; shift;; esac; done
set -- $args
side() { case $1 in ghcr.io/tkgstrator/commitguard/commitguard*) echo src;; *) echo dst;; esac; }
case $cmd in
  repo) [ "$1" = tags ] || exit 9; ls "$R/dst" ;;
  resolve) [ "$1" != "${FAIL_REF:-}" ] || exit 1; s=$(side "$1"); cat "$R/$s/${1##*:}" 2>/dev/null ;;
  manifest) [ "$1" = fetch ] || exit 9; shift; d=${1##*@}; [ "$d" != "$1" ] || d=$(cat "$R/$(side "$1")/${1##*:}"); cat "$R/man/$d" ;;
  copy) t=${2##*:}; echo "$t" >> "$R/writes"
        [ "$t" != "${FAIL_COPY_TAG:-}" ] || exit 1
        [ "$t" != latest ] || exit 1
        printf '%s\n' "${BAD_DIGEST:-${1##*@}}" > "$R/dst/$t"
        ;;
  *) exit 9 ;;
esac
MOCK
chmod +x "$tmp/bin/oras"
S=sha256:$(printf 'a%.0s' $(seq 64)); O=sha256:$(printf 'b%.0s' $(seq 64))
manifest() { # version [id] [mediaType]
  jq -n --arg v "$1" --arg i "${2:-commitguard}" --arg t "${3:-application/vnd.devcontainers}" \
    '{config:{mediaType:$t},annotations:{"dev.containers.metadata":({id:$i,version:$v}|tojson)}}'; }
setup() { # version
  REG=$tmp/reg; rm -rf "$REG"; mkdir -p "$REG/src" "$REG/dst" "$REG/man"; : > "$REG/writes"
  printf '%s\n' "$S" > "$REG/src/$1"; manifest "$1" > "$REG/man/$S"
  printf '%s\n' "$O" > "$REG/dst/latest"; manifest 0.9.0 > "$REG/man/$O"
  echo '{"id":"commitguard","version":"'"$1"'"}' > "$tmp/meta.json"
  export REG PATH="$tmp/bin:$PATH" ORAS_REGISTRY_CONFIG="$tmp/auth.json"; : > "$tmp/auth.json"; }
run() { sh "$root/scripts/publish-feature-alias.sh" "$tmp/meta.json" >"$tmp/out" 2>&1; }
fail() { echo "FAIL: $*" >&2; cat "$tmp/out" >&2; exit 1; }
ok() { run || fail "$1"; }
bad() { name=$1; run && fail "$name accepted"; [ ! -s "$REG/writes" ] || fail "$name wrote"; }
dst() { cat "$REG/dst/$1"; }

setup 1.2.3; ok success
for t in 1.2.3 1.2 1; do [ "$(dst $t)" = "$S" ] || fail "tag $t"; done
[ "$(dst latest)" = "$O" ] || fail 'latest touched'
ok idempotence
setup 1.2.3; printf '%s\n' "$O" > "$REG/dst/1.2.3"; bad 'exact-version collision'
setup 1.2.3; printf '%s\n' "$O" > "$REG/dst/1"; manifest 2.0.0 > "$REG/man/$O"; bad 'newer float'
setup 1.2.3; printf '%s\n' "$O" > "$REG/dst/1"; manifest 1.0.0 commitguard application/octet-stream > "$REG/man/$O"; bad 'non-Feature float'
setup 1.2.3; echo '{"id":"x","version":"1.2.3"}' > "$tmp/meta.json"; bad 'wrong id'
for v in 01.2.3 1.2 1.2.3-rc1; do setup 1.2.3; echo '{"id":"commitguard","version":"'$v'"}' > "$tmp/meta.json"; bad "version $v"; done
setup 1.2.3; manifest 1.2.4 > "$REG/man/$S"; bad 'source metadata version'
setup 1.2.3; manifest 1.2.3 other > "$REG/man/$S"; bad 'source id'
for c in repo resolve manifest; do setup 1.2.3; FAIL_ON=$c; export FAIL_ON; bad "$c failure"; unset FAIL_ON; done
setup 1.2.3; FAIL_REF=ghcr.io/tkgstrator/commitguard:latest; export FAIL_REF; bad 'collection resolve failure'; unset FAIL_REF
setup 1.2.3; manifest 1.1.0 > "$REG/man/$O"; printf '%s\n' "$O" > "$REG/dst/1.2"
FAIL_COPY_TAG=1; export FAIL_COPY_TAG; run && fail 'copy failure accepted'; unset FAIL_COPY_TAG
[ "$(dst 1.2)" = "$S" ] || fail 'successful tag was rolled back'
setup 1.2.3; BAD_DIGEST=$O; export BAD_DIGEST; run && fail 'digest mismatch accepted'; unset BAD_DIGEST
grep -qx latest "$REG/writes" && fail 'latest written'
echo 'test-feature-alias: ok'
