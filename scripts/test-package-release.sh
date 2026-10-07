#!/bin/sh
# Development regression harness. Never packages or installs into the checkout.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/commitguard-package-tests.XXXXXX")
trap 'rm -rf "$tmp"' 0
trap 'exit 129' 1
trap 'exit 130' 2
trap 'exit 143' 15
fail() { echo "test-package-release: $*" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  real_sha256=$(command -v sha256sum)
  sha256_kind=sha256sum
  digest() { value=$("$real_sha256" "$1") || return; printf '%s\n' "$value" | awk '{print $1}'; }
else
  real_sha256=$(command -v shasum)
  sha256_kind=shasum
  digest() { value=$("$real_sha256" -a 256 "$1") || return; printf '%s\n' "$value" | awk '{print $1}'; }
fi
target=aarch64-apple-darwin
baseline="$tmp/baseline"
mkdir -p "$baseline/scripts" "$baseline/Formula" "$baseline/bin/$target"
for file in Cargo.toml Cargo.lock install.sh LICENSE README.md; do cp "$root/$file" "$baseline/"; done
for dir in crates resources; do [ ! -d "$root/$dir" ] || cp -R "$root/$dir" "$baseline/"; done
for file in package-release.sh source-fingerprint.sh build.sh check commitlint; do
  cp "$root/scripts/$file" "$baseline/scripts/"
done
cp "$root/Formula/"*.rb "$baseline/Formula/"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$baseline/Cargo.toml" | head -n 1)
[ -n "$version" ] || fail 'workspace version missing'
for binary in commitguard gh-commit-guard commitlint; do
  printf '#!/bin/sh\necho "fixture %s"\n' "$version" > "$baseline/bin/$target/$binary"
  chmod 0755 "$baseline/bin/$target/$binary"
done
proof="$baseline/bin/$target/build-info"
{
  printf 'version=%s\nsource_sha256=%s\n' "$version" "$(/bin/sh "$baseline/scripts/source-fingerprint.sh" "$baseline")"
  for binary in commitguard gh-commit-guard commitlint; do
    printf '%s  %s\n' "$(digest "$baseline/bin/$target/$binary")" "$binary"
  done
} > "$proof"
second=x86_64-apple-darwin
cp -R "$baseline/bin/$target" "$baseline/bin/$second"
formula_state() {
  for file in "$1/Formula/"*.rb; do printf '%s  %s\n' "$(digest "$file")" "${file##*/}"; done
}
rejected() {
  name=$1
  fixture="$tmp/$name"
  cp -R "$baseline" "$fixture"
  case "$name" in
    missing-proof) rm "$fixture/bin/$target/build-info" ;;
    wrong-version) sed 's/^version=.*/version=invalid-fixture-version/' "$proof" > "$fixture/bin/$target/build-info" ;;
    missing-crates) rm -rf "$fixture/crates" ;;
    stale-source) printf '\n# changed after build\n' >> "$fixture/Cargo.toml" ;;
    changed-binary) printf '\n# unproven binary\n' >> "$fixture/bin/$target/commitguard" ;;
    missing-binary) rm "$fixture/bin/$target/commitguard" ;;
    invalid-second-target) rm "$fixture/bin/$second/build-info" ;;
  esac
  before=$(formula_state "$fixture")
  set -- "$target"
  [ "$name" != invalid-second-target ] || set -- "$target" "$second"
  if /bin/sh "$fixture/scripts/package-release.sh" "$@" > "$tmp/$name.log" 2>&1; then
    fail "$name was accepted"
  fi
  [ ! -e "$fixture/dist" ] || fail "$name created release output before rejection"
  [ "$(formula_state "$fixture")" = "$before" ] || fail "$name changed Formula before rejection"
  printf 'PASS %s rejected without release/Formula mutation\n' "$name"
}
for name in missing-proof wrong-version stale-source changed-binary missing-binary invalid-second-target missing-crates; do rejected "$name"; done
# Missing source trees must fail the fingerprint itself, without emitting a hash.
if /bin/sh "$tmp/missing-crates/scripts/source-fingerprint.sh" "$tmp/missing-crates" > "$tmp/missing-fingerprint.out" 2> "$tmp/missing-fingerprint.err"; then
  fail 'missing source tree produced a successful fingerprint'
fi
[ ! -s "$tmp/missing-fingerprint.out" ] || fail 'failed source enumeration emitted a fingerprint'
printf 'PASS source enumeration failure propagates without a fingerprint\n'

# Replace an already-validated binary exactly when the last original binary is
# hashed. This deterministically exercises the validation-to-staging race.
tools="$tmp/race-tools"
mkdir "$tools"
cat > "$tools/sha256sum" <<'SH'
#!/bin/sh
set -eu
if [ "$REAL_SHA256_KIND" = shasum ]; then
  result=$("$REAL_SHA256" -a 256 "$@")
else
  result=$("$REAL_SHA256" "$@")
fi
printf '%s\n' "$result"
if [ "${1:-}" = "$PACKAGE_RACE_FIXTURE/bin/aarch64-apple-darwin/commitlint" ]; then
  case "$PACKAGE_RACE_MODE" in
    binary) printf '\n# concurrently replaced\n' >> "$PACKAGE_RACE_FIXTURE/bin/aarch64-apple-darwin/commitguard" ;;
    source) printf '\n# concurrently changed\n' >> "$PACKAGE_RACE_FIXTURE/Cargo.toml" ;;
  esac
fi
SH
chmod 0755 "$tools/sha256sum"
for mode in binary source; do
  fixture="$tmp/concurrent-$mode"
  cp -R "$baseline" "$fixture"
  race_root=$(CDPATH= cd -- "$fixture" && pwd)
  before=$(formula_state "$fixture")
  if PATH="$tools:$PATH" REAL_SHA256="$real_sha256" REAL_SHA256_KIND="$sha256_kind" \
    PACKAGE_RACE_FIXTURE="$race_root" PACKAGE_RACE_MODE="$mode" \
    /bin/sh "$fixture/scripts/package-release.sh" "$target" > "$tmp/concurrent-$mode.log" 2>&1; then
    fail "concurrent $mode replacement was accepted"
  fi
  [ ! -e "$fixture/dist" ] || fail "concurrent $mode replacement created release output"
  [ "$(formula_state "$fixture")" = "$before" ] || fail "concurrent $mode replacement changed Formula"
  case "$mode" in
    binary) grep -Fq 'payload changed while staging' "$tmp/concurrent-$mode.log" || fail 'binary race rejected for the wrong reason' ;;
    source) grep -Fq 'source changed while staging' "$tmp/concurrent-$mode.log" || fail 'source race rejected for the wrong reason' ;;
  esac
  printf 'PASS concurrent %s replacement rejected before release/Formula mutation\n' "$mode"
done
fixture="$tmp/valid"
cp -R "$baseline" "$fixture"
before=$(formula_state "$fixture")
/bin/sh "$fixture/scripts/package-release.sh" "$target" > "$tmp/valid.log" 2>&1 || {
  cat "$tmp/valid.log" >&2
  fail 'valid proof rejected'
}
[ -f "$fixture/dist/partial/v$version/$target/commitlint-rust-$target.tar.gz" ] || fail 'partial combined archive missing'
[ "$(formula_state "$fixture")" = "$before" ] || fail 'partial build changed universal Formula'
printf 'PASS valid partial build packages without universal Formula mutation\n'
archive_members() { tar -tzf "$1" | LC_ALL=C sort; }
dist="$fixture/dist/partial/v$version/$target"
[ "$(archive_members "$dist/commitlint-rust-$target.tar.gz")" = "$(printf '%s\n' LICENSE README.md commitlint gh-commit-guard)" ] || fail 'combined archive breaks original installer contract'
[ "$(archive_members "$dist/commitguard-$target.tar.gz")" = "$(printf '%s\n' LICENSE README.md commitguard)" ] || fail 'guard-only archive has unexpected members'
[ "$(archive_members "$dist/commitlint-only-$target.tar.gz")" = "$(printf '%s\n' LICENSE README.md commitlint)" ] || fail 'lint-only archive has unexpected members'
printf 'PASS combined and independent archive member contracts\n'

fixture="$tmp/all-targets"
cp -R "$baseline" "$fixture"
for platform in aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
  cp -R "$fixture/bin/$target" "$fixture/bin/$platform"
done
/bin/sh "$fixture/scripts/package-release.sh" > "$tmp/all-targets.log" 2>&1 || {
  cat "$tmp/all-targets.log" >&2
  fail 'valid all-target packaging rejected'
}
dist="$fixture/dist/v$version"
[ "$(wc -l < "$dist/SHA256SUMS" | tr -d ' ')" -eq 13 ] || fail 'all-target checksums omit expected assets'
while read -r expected archive; do
  [ "$(digest "$dist/$archive")" = "$expected" ] || fail "all-target archive checksum differs: $archive"
done < "$dist/SHA256SUMS"
for kind in commitlint commitlint-rust commitguard; do
  formula="$fixture/Formula/$kind.rb"
  [ "$(grep -c 'sha256 "' "$formula")" -eq 4 ] || fail "$kind Formula omits a platform"
  grep -Fq "version \"$version\"" "$formula" || fail "$kind Formula has stale version"
done
grep -Fq 'commitlint-only-' "$fixture/Formula/commitlint.rb" || fail 'lint Formula uses combined distribution'
if grep -Fq 'depends_on' "$fixture/Formula/commitlint.rb"; then fail 'lint Formula adds runtime dependencies'; fi
grep -Fq 'commitlint-rust-' "$fixture/Formula/commitlint-rust.rb" || fail 'legacy Formula lost its combined distribution'
grep -Fq 'bin.install "gh-commit-guard"' "$fixture/Formula/commitlint-rust.rb" || fail 'legacy upgrade loses guard command'
grep -Fq 'bin.install "commitlint" => "commitlint-rust"' "$fixture/Formula/commitlint-rust.rb" || fail 'legacy upgrade loses lint command'
grep -Fq 'bin.install_symlink "gh-commit-guard" => "commitguard"' "$fixture/Formula/commitlint-rust.rb" || fail 'legacy Formula omits canonical alias'
grep -Fq 'conflicts_with "commitguard"' "$fixture/Formula/commitlint-rust.rb" || fail 'legacy Formula omits guard conflict'
grep -Fq 'conflicts_with "commitlint-rust"' "$fixture/Formula/commitguard.rb" || fail 'guard Formula omits legacy conflict'
grep -Fq 'depends_on "gh"' "$fixture/Formula/commitguard.rb" || fail 'guard Formula omits gh dependency'
grep -Fq 'depends_on "git"' "$fixture/Formula/commitguard.rb" || fail 'guard Formula omits Git dependency'
grep -Fq 'bin.install_symlink "commitguard" => "gh-commit-guard"' "$fixture/Formula/commitguard.rb" || fail 'guard Formula omits compatibility entry'
printf 'PASS all-target assets/checksums and independent universal Formulae\n'
before_sums=$(digest "$dist/SHA256SUMS")
before_formula=$(formula_state "$fixture")
/bin/sh "$fixture/scripts/package-release.sh" "$target" > "$tmp/partial-after-full.log" 2>&1 || fail 'partial after full failed'
[ "$(digest "$dist/SHA256SUMS")" = "$before_sums" ] || fail 'partial overwrote full release checksums'
[ "$(formula_state "$fixture")" = "$before_formula" ] || fail 'partial overwrote universal Formulae'
printf 'PASS partial after full preserves release and upgrade compatibility\n'
