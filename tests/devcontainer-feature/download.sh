#!/bin/sh
# Test malformed downloads with the production installer inside a disposable image.
# Every negative corrupts one payload while the other stays valid, and both
# installed binaries plus feature state must stay unchanged.
set -eu
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir "$fixture/tools" "$fixture/source" "$fixture/work"
cp /source/commitguard/* "$fixture/source/"
cat > "$fixture/tools/curl" <<'CURL'
#!/bin/sh
while [ "$#" -gt 0 ]; do
  if [ "$1" = --output ]; then destination=$2; shift 2; else url=$1; shift; fi
done
case "${url##*/}" in
  commitguard-*) cp "$TEST_GUARD" "$destination" ;;
  commitlint-only-*) cp "$TEST_LINT" "$destination" ;;
  *) echo "unexpected URL: $url" >&2; exit 22 ;;
esac
CURL
chmod +x "$fixture/tools/curl"
PATH="$fixture/tools:$PATH"; export PATH

snapshot() {
  sha256sum /usr/local/bin/commitguard /usr/local/bin/commitlint-rust
  readlink /usr/local/bin/gh-commit-guard
  readlink /usr/local/bin/commitguard-devcontainer-setup
  sha256sum /etc/profile.d/commitguard-feature.sh
  find /usr/local/bin -maxdepth 1 -name '.commit*.new' -exec ls -ldn --time-style=+ {} + | LC_ALL=C sort
  find /usr/local/share/commitguard -exec ls -ldn --time-style=+ {} + | LC_ALL=C sort
  find /usr/local/share/commitguard -type f -exec sha256sum {} + | LC_ALL=C sort
}
before=$(snapshot)

# make_archive OUT MEMBER KIND: valid, extra, symlink or traversal archive.
make_archive() (
  out=$1 member=$2 kind=$3
  rm -rf "$fixture/work"; mkdir "$fixture/work"
  printf 'license\n' > "$fixture/work/LICENSE"
  printf 'readme\n' > "$fixture/work/README.md"
  members="$member LICENSE README.md"
  case "$kind" in
    symlink) ln -s /etc/passwd "$fixture/work/$member" ;;
    *) printf 'binary\n' > "$fixture/work/$member" ;;
  esac
  case "$kind" in
    extra) printf 'extra\n' > "$fixture/work/extra"; members="$members extra" ;;
  esac
  if [ "$kind" = traversal ]; then
    tar -czf "$out" --transform="s|$member|../$member|" -C "$fixture/work" $members
  else
    tar -czf "$out" -C "$fixture/work" $members
  fi
)
sha() { d=$(sha256sum "$1"); echo "${d%% *}"; }

# run_case PAYLOAD KIND: PAYLOAD (guard|lint) is bad, the other one valid.
run_case() {
  payload=$1 kind=$2
  make_archive "$fixture/guard.tar.gz" commitguard valid
  make_archive "$fixture/lint.tar.gz" commitlint valid
  case "$payload" in guard) member=commitguard ;; lint) member=commitlint ;; esac
  if [ "$kind" = corrupt ]; then
    printf 'corrupt\n' > "$fixture/$payload.tar.gz"
  else
    make_archive "$fixture/$payload.tar.gz" "$member" "$kind"
  fi
  cp /source/commitguard/install.sh "$fixture/source/install.sh"
  # Only the disposable copy's explicit per-archive trust roots change; the
  # corrupt payload keeps its real production digest.
  for name in guard lint; do
    [ "$kind" = corrupt ] && [ "$name" = "$payload" ] && continue
    sed -i -E "s/(${name}_digest=)[0-9a-f]{64}/\1$(sha "$fixture/$name.tar.gz")/g" "$fixture/source/install.sh"
  done
  TEST_GUARD="$fixture/guard.tar.gz" TEST_LINT="$fixture/lint.tar.gz"; export TEST_GUARD TEST_LINT
  if "$fixture/source/install.sh" > "$fixture/error" 2>&1; then cat "$fixture/error"; exit 1; fi
  case "$kind" in
    corrupt) grep -q 'checksum mismatch' "$fixture/error" ;;
    extra|traversal) grep -q 'archive must contain exactly' "$fixture/error" ;;
    symlink) grep -q 'regular files' "$fixture/error" ;;
  esac
  case "$payload" in
    guard) grep -q 'commitguard-' "$fixture/error" ;;
    lint) grep -q 'commitlint-only-' "$fixture/error" ;;
  esac
  [ "$(snapshot)" = "$before" ] || { echo "state changed: $payload $kind" >&2; exit 1; }
  echo "PASS $payload $kind archive rejected before any installation"
}

for payload in guard lint; do
  for kind in corrupt extra symlink traversal; do
    run_case "$payload" "$kind"
  done
done
