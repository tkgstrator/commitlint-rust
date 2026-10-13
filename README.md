# commitlint-rust and commitguard

Rust tools for Conventional Commits and Git identity checks:

- **commitlint-rust** checks commit messages. The source implements all 38 rules from Commitlint 21.2.3.
- **commitguard** checks Author/Committer against your authenticated GitHub account and enforces the commit policy through guarded Git and hooks. Git and gh are its runtime dependencies.

No Rust compiler, Bun or Python is needed to run the published binaries.

## Install

Authenticate with GitHub, then install the guard and Codex/Claude skills without cloning:

```sh
gh auth login --hostname github.com
curl -fsSL https://github.com/tkgstrator/commitlint-rust/releases/latest/download/install.sh | bash
```

Installation changes user-level Git, shell and agent configuration. Reopen your terminal afterward. For the standalone linter, Homebrew and installation options, see [INSTALL.md](INSTALL.md).

The installer currently installs published v0.2.0, which supports fewer lint rules than the current source. Source-only configuration and history-rewrite features require a source build.

## Dev Containers

Install both tools, Git and gh with one Feature on Debian/Ubuntu (amd64/arm64). It pins published v0.2.0 with its limited lint rule set:

```json
{
  "features": {
    "ghcr.io/tkgstrator/commitguard:1": {}
  }
}
```

The guard activates automatically when the container starts. Run `gh auth login -h github.com` before committing or pushing. Installed commands include `commitguard` and the standalone `commitlint-rust`. See [Feature options](features/commitguard/README.md).

## Use

With the standalone linter installed (`commitlint-rust` in the Feature, `commitlint` in the independent archive/Homebrew package):

```sh
printf 'feat: add search\n' | commitlint-rust
```

Check the guard account or a proposed commit message:

```sh
commitguard account
commitguard message message.txt
```

The guard requires English printable ASCII Conventional Commits, at most **128 characters for the entire message**, and matching authenticated GitHub Author/Committer identities. Recognized AI attribution is preserved. Published v0.2.0 verifies identity online on every check.

All 38 rules are available through the source build's [JSON configuration](docs/reference.md#all-rule-json-configuration-source-build). JavaScript configuration, plugins and custom parser expressions are unsupported.

## Documentation

- [Installation and release instructions](INSTALL.md)
- [Detailed configuration, compatibility and development reference](docs/reference.md)
- [Unpublished history repair](docs/superpowers/specs/2026-10-07-commitguard-fix.md)
- [Fast bulk history rewrite](docs/superpowers/specs/2026-10-07-commitguard-bulk-write.md)

## Development

```sh
cargo test --workspace --locked
cargo build --workspace --locked --release
```

CI tests and builds Linux/macOS on x86_64 and ARM64. See [CI/CD](INSTALL.md#cicd) for release publishing.
