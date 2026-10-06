# Install without a source checkout

Git and authenticated GitHub CLI (`gh auth login --hostname github.com`) are required. macOS/Linux standard shell tools, tar/gzip, mktemp and either sha256sum or shasum are used during bootstrap. The installed guard does not require Rust, Bun, Python, curl or a source checkout.

Run this single command from any directory:

```sh
sh -c 'unset GH_DEBUG DEBUG; d=$(mktemp -d) || exit; trap '\''rm -rf "$d"'\'' 0; gh release download "${COMMITLINT_RUST_VERSION:-v0.1.0}" -R github.com/tkgstrator/commitlint-rust -p install.sh --dir "$d" && sh "$d/install.sh" "$@"' --
```

It downloads the release installer to a temporary file and runs it only after download succeeds. The installer selects the platform, downloads a pinned release binary archive, checks SHA256 before extraction, then installs user-wide Git guards and Codex/Claude skills. Reopen your shell afterward. Existing hooks/configuration are retained with the native installer's rollback protections. No credentials are copied. No sudo is used.

This command is an explicit opt-in to changes in your user Git, shell and agent configuration. Finish any active history repair before replacing its installed guard. To install portable skills without activating global Git/shell settings, append `--skills-only` after the final `--`. Native `--home`, `--container --repo` and other supported setup flags are forwarded unchanged. `COMMITLINT_RUST_VERSION=v0.1.0` can select a release tag; the downloaded installer pins its default release version.

The bootstrap trusts the repository and release publisher, including the downloaded script. Checksums detect corrupted/wrong archives; they do not independently authenticate a compromised publisher. Unsupported platforms, missing assets/tools, failed authentication/downloads and checksum mismatches stop before activation.

## Homebrew

The Formula is in this repository, so use an explicit tap URL:

```sh
brew tap tkgstrator/commitlint-rust https://github.com/tkgstrator/commitlint-rust && brew install tkgstrator/commitlint-rust/commitlint-rust
```

This installs `commitlint-rust` and `gh-commit-guard`. Homebrew manages its own tap checkout; you do not clone or compile a project manually. Global guard activation is explicit:

```sh
gh-commit-guard install
```

Homebrew installation and upgrades do not automatically modify user Git/agent settings. After an explicitly intended guard upgrade, rerun its setup only after active history repairs finish. The standalone `commitlint-rust` command accepts stdin, `--edit [FILE]` and `--from REF --to REF` without gh authentication.

## Maintainer release steps

Build and verify all target binaries, update the pinned installer version with Cargo.toml, then run `sh scripts/package-release.sh`. The script creates `dist/v<version>/` archives, install.sh and SHA256SUMS, and generates the Formula with exact archive digests. Commit the reviewed source and Formula, normally push the exact checked source ref, and create a release from that exact commit. Upload all generated assets together, preferably through a draft release before publication. Verify the downloaded assets and execute setup against an isolated HOME after publishing. Never overwrite a public version's assets with a different build.
