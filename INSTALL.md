# Install commitguard without a source checkout

Git and authenticated GitHub CLI (`gh auth login --hostname github.com`) are required for commitguard. Standard OS shell tools, tar/gzip, mktemp and sha256sum or shasum are used during bootstrap. Installed runtime needs no Rust, Bun, Python or source checkout.

The currently published v0.1.0 release can still be installed with:

```sh
sh -c 'unset GH_DEBUG DEBUG; d=$(mktemp -d) || exit; trap '\''rm -rf "$d"'\'' 0; gh release download "${COMMITLINT_RUST_VERSION:-v0.1.0}" -R github.com/tkgstrator/commitlint-rust -p install.sh --dir "$d" && sh "$d/install.sh" "$@"' --
```

Source v0.2.0 prepares independent packages and the canonical `commitguard` name. Its installer/release assets must be published together before using the v0.2.0 URL; a local build does not change the existing public release. Once that release is published, Git+gh can download and run its installer without curl:

```sh
sh -c 'unset GH_DEBUG DEBUG; d=$(mktemp -d) || exit; trap '\''rm -rf "$d"'\'' 0; gh release download "${COMMITLINT_RUST_VERSION:-v0.2.0}" -R github.com/tkgstrator/commitlint-rust -p install.sh --dir "$d" && sh "$d/install.sh" "$@"' --
```

The bootstrap checks SHA256 and the exact archive members before extracting and executing the native guard. Native setup installs both `commitguard` and `gh-commit-guard`, changes user Git/shell/agent configuration, preserves hooks/verifiers and has rollback protections. This is an explicit opt-in. Reopen shells afterward. Finish active history repairs before replacing their installed guard. No credentials are copied and no sudo is used.

Append `--skills-only` after the final `--` to install portable skills without global Git/shell activation. Other native arguments, such as `--home` and `--container --repo`, are forwarded unchanged. A pinned bootstrap can select a different release using `COMMITLINT_RUST_VERSION`; the combined archive retains its four-member contract for archived installers.

The script and checksums trust the repository/release publisher. Unsupported platforms, missing tools/assets, download/authentication failures and checksum mismatches stop before activation.

## Independent binaries and Homebrew

The message-only archive `commitlint-only-TARGET.tar.gz` contains `commitlint`, LICENSE and README.md. Stdin and explicit file linting need no external tools; implicit Git message paths and revision ranges need Git. It never queries gh or reads guard configuration.

The guard-only archive `commitguard-TARGET.tar.gz` contains `commitguard`, LICENSE and README.md. Its message checker is compiled in. Run `commitguard install` explicitly to install guarded Git and agent skills; simply downloading the command does not activate global policy.

When v0.2.0 assets and Formulae are published, new installations can choose independent formulae:

```sh
brew tap tkgstrator/commitlint-rust https://github.com/tkgstrator/commitlint-rust
brew install tkgstrator/commitlint-rust/commitlint
brew install tkgstrator/commitlint-rust/commitguard
commitguard install
```

The `commitlint` Formula has no gh dependency or setup action. `commitguard` requires Git and gh and also exposes `gh-commit-guard`. The existing `commitlint-rust` Formula remains a combined compatibility package: upgrading it preserves `commitlint-rust` and `gh-commit-guard`, and adds the canonical `commitguard` alias. It conflicts with the separate guard package because both own the same guard commands; existing users can keep the combined package. Homebrew installs/upgrades do not activate policy automatically.

The working-tree Formulae are release candidates pointing to unpublished v0.2.0 assets. Publish the reviewed assets before exposing these Formulae on the public tap branch. The currently published tap remains at v0.1.0 until that coordinated rollout.

An explicit downgrade with the archived v0.1.0 installer does not manage the new `commitguard` path. After such a downgrade, use `gh-commit-guard` until reinstalling the newer version; the stale canonical copy must not be treated as the downgraded guard.

## Maintainer release steps

Run `sh scripts/build.sh [target]` for every supported target. It tests/builds both workspace packages and records source/payload SHA256 build provenance. A cross-build without an executable runner may set `COMMITGUARD_CROSS_BUILD_ONLY=1`; report that target as compile-only and test native targets separately.

Run `sh scripts/test-package-release.sh` and then `sh scripts/package-release.sh`. Packaging checks all build proofs against current source and binary digests before writing output, creates combined/guard-only/message-only archives and the pinned installer in `dist/v<version>/`, and generates independent plus compatibility Formulae from exact archive hashes. Explicit target arguments write to `dist/partial/v<version>/<selected-targets>/`, leaving full-release assets/checksums and universal Formulae intact; this is insufficient for a complete release.

Review and commit source, Formulae, native bundles and manifest together. Normally push the checked source ref, create a draft release at that exact commit, and upload all assets before publication. Verify downloaded assets and isolated setup afterward. Never overwrite a public version's assets with different builds or alter the live guard while active repair jobs bind its hashes.
