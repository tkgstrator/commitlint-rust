#!/bin/sh
# Test malformed downloads with the production installer inside a disposable image.
set -eu
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir "$fixture/tools" "$fixture/source" "$fixture/payload"
cp /source/commitguard/* "$fixture/source/"
cat > "$fixture/tools/curl" <<'CURL'
#!/bin/sh
while [ "$#" -gt 0 ]; do
  if [ "$1" = --output ]; then destination=$2; shift 2; else shift; fi
done
cp "$TEST_ARCHIVE" "$destination"
CURL
chmod +x "$fixture/tools/curl"
PATH="$fixture/tools:$PATH"; export PATH
before=$(sha256sum /usr/local/bin/commitguard)
printf 'corrupt\n' > "$fixture/corrupt"
TEST_ARCHIVE="$fixture/corrupt"; export TEST_ARCHIVE
if "$fixture/source/install.sh" > "$fixture/error" 2>&1; then exit 1; fi
grep -q 'checksum mismatch' "$fixture/error"
[ "$(sha256sum /usr/local/bin/commitguard)" = "$before" ]
echo 'PASS corrupt download rejected before installation'
printf 'binary\n' > "$fixture/payload/commitguard"
printf 'license\n' > "$fixture/payload/LICENSE"
printf 'readme\n' > "$fixture/payload/README.md"
printf 'extra\n' > "$fixture/payload/extra"
for kind in extra symlink traversal; do
  cp /source/commitguard/install.sh "$fixture/source/install.sh"
  rm -f "$fixture/payload/commitguard"
  case "$kind" in
    symlink) ln -s /etc/passwd "$fixture/payload/commitguard"; members='commitguard LICENSE README.md';;
    extra) printf 'binary\n' > "$fixture/payload/commitguard"; members='commitguard LICENSE README.md extra';;
    traversal) printf 'binary\n' > "$fixture/payload/commitguard"; members='commitguard LICENSE README.md';;
  esac
  if [ "$kind" = traversal ]; then
    tar -czf "$fixture/invalid.tar.gz" --transform='s|commitguard|../commitguard|' -C "$fixture/payload" $members
  else
    tar -czf "$fixture/invalid.tar.gz" -C "$fixture/payload" $members
  fi
  digest=$(sha256sum "$fixture/invalid.tar.gz"); digest=${digest%% *}
  # Only a disposable test copy changes its trust root to exercise the contract.
  sed -i -E "s/digest=[0-9a-f]{64}/digest=$digest/g" "$fixture/source/install.sh"
  TEST_ARCHIVE="$fixture/invalid.tar.gz"
  if "$fixture/source/install.sh" > "$fixture/error" 2>&1; then cat "$fixture/error"; exit 1; fi
  grep -Eq 'archive must contain exactly|regular files' "$fixture/error"
  [ "$(sha256sum /usr/local/bin/commitguard)" = "$before" ]
  echo "PASS $kind archive rejected before extraction or installation"
done
