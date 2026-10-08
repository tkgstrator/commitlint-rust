# Install commitguard without a source checkout

Git and authenticated GitHub CLI (`gh auth login --hostname github.com`) are required for commitguard. Standard OS shell tools, tar/gzip, mktemp and sha256sum or shasum are used during bootstrap. Installed runtime needs no Rust, Bun, Python or source checkout.

The published v0.2.0 release can be installed directly with Git and gh, without curl or a source checkout:

```sh
sh -c 'unset GH_DEBUG DEBUG; d=$(mktemp -d) || exit; trap '\''rm -rf "$d"'\'' 0; gh release download "${COMMITLINT_RUST_VERSION:-v0.2.0}" -R github.com/tkgstrator/commitlint-rust -p install.sh --dir "$d" && sh "$d/install.sh" "$@"' --
```

The bootstrap checks SHA256 and the exact archive members before extracting and executing the native guard. Native setup installs both `commitguard` and `gh-commit-guard`, changes user Git/shell/agent configuration, preserves hooks/verifiers and has rollback protections. This is an explicit opt-in. Reopen shells afterward. Finish active history repairs before replacing their installed guard. No credentials are copied and no sudo is used.

Append `--skills-only` after the final `--` to install portable skills without global Git/shell activation. Other native arguments, such as `--home` and `--container --repo`, are forwarded unchanged. A pinned bootstrap can select a different release using `COMMITLINT_RUST_VERSION`; the combined archive retains its four-member contract for archived installers.

The script and checksums trust the repository/release publisher. Unsupported platforms, missing tools/assets, download/authentication failures and checksum mismatches stop before activation.

## Independent binaries and Homebrew

The message-only archive `commitlint-only-TARGET.tar.gz` contains `commitlint`, LICENSE and README.md. Stdin and explicit file linting need no external tools; implicit Git message paths and revision ranges need Git. It never queries gh or reads guard configuration.

The guard-only archive `commitguard-TARGET.tar.gz` contains `commitguard`, LICENSE and README.md. Its message checker is compiled in. Run `commitguard install` explicitly to install guarded Git and agent skills; simply downloading the command does not activate global policy.

New installations can choose the independent v0.2.0 formulae:

```sh
brew tap tkgstrator/commitlint-rust https://github.com/tkgstrator/commitlint-rust
brew install tkgstrator/commitlint-rust/commitlint
brew install tkgstrator/commitlint-rust/commitguard
commitguard install
```

The `commitlint` Formula has no gh dependency or setup action. `commitguard` requires Git and gh and also exposes `gh-commit-guard`. The existing `commitlint-rust` Formula remains a combined compatibility package: upgrading it preserves `commitlint-rust` and `gh-commit-guard`, and adds the canonical `commitguard` alias. It conflicts with the separate guard package because both own the same guard commands; existing users can keep the combined package. Homebrew installs/upgrades do not activate policy automatically.

The Formulae reference the immutable v0.2.0 assets. Unreleased source changes require a new coordinated version, installer tag, assets and Formulae; rebuilding source does not update the published release.

An explicit downgrade with the archived v0.1.0 installer does not manage the new `commitguard` path. After such a downgrade, use `gh-commit-guard` until reinstalling the newer version; the stale canonical copy must not be treated as the downgraded guard.

## Maintainer release steps

Run `sh scripts/build.sh [target]` for every supported target. It tests/builds both workspace packages and records source/payload SHA256 build provenance. A cross-build without an executable runner may set `COMMITGUARD_CROSS_BUILD_ONLY=1`; report that target as compile-only and test native targets separately.

Run `sh scripts/test-package-release.sh` and then `sh scripts/package-release.sh`. Packaging checks all build proofs against current source and binary digests before writing output, creates combined/guard-only/message-only archives and the pinned installer in `dist/v<version>/`, and generates independent plus compatibility Formulae from exact archive hashes. Explicit target arguments write to `dist/partial/v<version>/<selected-targets>/`, leaving full-release assets/checksums and universal Formulae intact; this is insufficient for a complete release.

## CI/CD

The workflows follow the Integration/Deployment layout of the devcontainers Rust example. [Integration](.github/workflows/integration.yaml) checks formatting, Clippy, outgoing commit messages and the pinned upstream oracle. Its four native runners test and build Linux musl and macOS binaries on both x86_64 and ARM64. Packaging restores executable bits after artifact transfer, checks source/payload provenance, runs packaging regression tests and creates 12 archives, the installer and SHA256SUMS. Generated Homebrew Formulae and build provenance are retained as separate artifacts.

[Deployment](.github/workflows/deployment.yaml) repeats Integration for the tagged source and publishes only after it succeeds. A pushed existing `v<version>` tag must match Cargo.toml and the installer's pinned release tag; an existing GitHub Release or uncertain release lookup stops publication. Release creation uses `--verify-tag`, never creates a Git tag and never overwrites existing assets. Manual workflow dispatch builds and validates without publishing.

For a new release, review and commit the coordinated source/version/installer changes, then create and push the authorized version tag. Download and review the generated Formulae before a separate guarded commit updates the tap; the workflow does not create commits or update Formulae automatically. Verify downloaded assets and isolated setup afterward. Never overwrite a public version's assets with different builds or alter the live guard while active repair jobs bind its hashes.
