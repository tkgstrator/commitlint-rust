# Commitlint / commitguard separation

The owner approved independent Rust crates in the existing repository, with
commitguard embedding the message checker and preserving the existing installed
guard. This is a source and release-preparation change; no active host setup,
consumer checkout, public tag or already published release is replaced here.

## Responsibilities and dependency direction

- `commitlint-rust`: message-only library and `commitlint` executable. Preserve
  the supported Conventional Commit rules, printable ASCII/LF policy and the
  128-character entire-message limit, plus the 221 Commitlint oracle cases.
  Stdin and explicit message files need no Git or gh. Implicit edit paths and
  commit ranges need Git only. No account, identity, attribution, hook, setup or
  Git proxy dependencies belong in this crate.
- `commitguard`: fresh human gh authentication, raw Author/Committer checks,
  recognized AI/human attribution policy, actual outgoing history checks,
  signing restrictions, hook chaining, guarded Git and transactional setup.
  Embed the lint library at build time; no runtime commitlint process required.
  Git and gh are required, with the platform shell for Git hooks. Bun, Python,
  Cargo and Rust are development tools only.
- One Cargo workspace owns both crates. Dependencies flow from guard to lint;
  no reverse dependency or identity-bearing shared utility is allowed.

## Compatibility

Expose `commitguard` as the public guard executable, preserving the
`gh-commit-guard` executable and all prior commands/options/exit conventions as
a compatibility entry point. Existing hook scripts, generated skill checkers,
guard root, configuration schema, shell blocks and verifier settings remain
usable. Installing the new build in an isolated home produces both executable
names. Existing installed legacy config migrations and repo hook chaining must
continue passing their tests.

Keep archived v0.1.0 assets immutable. Prepare source release v0.2.0. A new
release's combined archive retains its existing `commitlint-rust-TARGET.tar.gz`
name and exact four-member payload (`gh-commit-guard`, `commitlint`, LICENSE,
README.md), so even the archived installer can select the newer release.
Native setup adds the canonical `commitguard` command transactionally while
generated hooks/checkers retain their legacy paths. The installer checks SHA256
before extraction. Separately package guard-only and message-only archives so lint consumers
can install without gh or guard activation. Formulae split responsibilities:
commitlint has no gh dependency; commitguard depends on Git and gh. The existing
commitlint-rust Formula remains a combined compatibility distribution so an
upgrade never silently removes its old commands. Existing
release URLs continue to resolve their original assets.

## Verification and acceptance

The workspace test suite, native install/rollback/migration tests, outgoing
commit tests, guarded bypass refusal tests and signing tests must pass. Add
tests for the canonical executable, compatible entry point, malformed guard
configuration having no effect on standalone lint, and stdin/file linting with
no Git/gh on PATH. Verify the message crate's dependency closure excludes the
guard and identity/config libraries. Test updated archive validation against
extra/duplicate/unsafe members; retain the archived installer's exact payload
contract when selecting newer releases. Never weaken the fixed policy to achieve parity.

Build and smoke-test the native Mac artifact, with other targets compiled or
limitations explicitly reported. Source artifacts must not accidentally ship
the old checked-in binaries as the new release. Inspect the actual diff with
an independent reviewer before declaring the source change complete.
