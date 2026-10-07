# Verification — 2026-10-07, source v0.2.0

- Independent workspace packages: `commitlint-rust` (message library/CLI) and `commitguard` (guard library, canonical/legacy CLI). Guard embeds lint; the normal lint dependency tree contains only regex/libc and their dependencies, with no guard/gh/config dependency.
- Native macOS ARM and Linux ARM64/x86_64 musl: **78 tests passed** on each platform. Two ignored entries are subprocess fixtures explicitly invoked by parent tests. Release builds and all three executable version checks passed.
- macOS Intel: cross-build succeeded. Its cached toolchain's missing LLVM lookup link was repaired for stripping; the final build completed without that warning. No Rosetta runtime verification is claimed.
- All **221** upstream Commitlint oracle cases pass. The moved fixture is byte-identical to v0.1.0, SHA256 `642c0ec0e7cee4e5f8407e9bec79e10d21f8ec7a8134c2a193c9d1a7cfeb47b4`.
- Packaging regression harness: **14 checks passed**, including missing/stale build proof, modified payloads, failed source enumeration, concurrent source/binary replacement, exact archive contents, independent/legacy Formulae, full checksums and partial-after-full isolation.
- Twelve native payloads match `bin/manifest.json`. Four combined, four message-only and four guard-only archives plus installer match `dist/v0.2.0/SHA256SUMS`.
- Actual release archive bootstrap installed both guard names and both agent skill copies into an isolated HOME/config. Both names run after temporary extraction cleanup. Message-only release lint runs with empty PATH. The source release guard's real `account` check returned the authenticated human `tkgstrator`.
- New tests verify canonical/legacy behavior, protected canonical install paths, transactional rollback and reinstall, configuration-free standalone lint, replacement-object refusal, an actual sleeping Git timeout, and range reads through real installed guarded Git with gh unavailable.

Independent native plan, Rust source and packaging reviews completed. Packaging findings were reproduced and corrected, then re-reviewed. Claude Code MCP completed the plan review, implemented the Rust split, and completed an independent Opus actual-diff review. No policy weakening was found. The vacuous timeout fixture was corrected and verified; legacy Homebrew upgrade commands are preserved by the compatibility Formula. A focused Sonnet follow-up found no blocking packaging issue. Its staging suggestion was applied so all Formula text is generated before tracked files are promoted. LocalGPT GPT 6 Pro's file-access check failed with `response_recovery_failed`, so no LocalGPT file review is claimed. The initial Claude environment lacked Cargo; Codex performed all native test/build checks.

Build proof records source/payload freshness, not a test attestation; platform execution coverage is reported above. Partial distributions have their own directory. Source archives can be rebuilt locally, and releases must upload only the entries listed in SHA256SUMS. The tap-qualified formula names avoid implicit package-name selection. Archived installer downgrades do not manage the new canonical path, as documented in INSTALL.md.

These source-preparation checks ran before publication and changed local source/distribution files only. At that checkpoint the installed v0.1.0 host guard, its config, global Git config and installed skill hash were unchanged, as were consumer checkouts and published v0.1.0 assets; no source commits or pushes had been made. Publication and host rollout are a separate step, verified against the exact release commit and downloaded assets.

---

# Historical verification — 2026-10-06, v0.1.0

Canonical source: this repository. Version: 0.1.0. Consumers: `claude-plugins/plugins/devflow/skills/gh-commit-identity`, `devcontainers/scripts/gh-commit-identity` and all 12 independently copyable templates.

## Executed checks

- Native macOS ARM: 13 unit tests and 41 integration tests passed; release binaries built.
- Linux ARM64 and x86_64 containers: the same 13 unit and 41 integration tests passed; static musl release binaries built.
- macOS Intel: official matching toolchain cross-build completed. Execution was unavailable because this Mac has no Rosetta; no runtime success is claimed for that target.
- Pinned upstream Commitlint 21.2.3 regenerated 221 cases: 139 accepted, 82 rejected. The fixture remained byte-identical, SHA256 `642c0ec0e7cee4e5f8407e9bec79e10d21f8ec7a8134c2a193c9d1a7cfeb47b4`. Rust validity matches all cases.
- Runtime integration fixtures expose only real Git and fixture gh on PATH; no Bun or Python is called. The one ignored unit entry is a subprocess fixture, explicitly executed by its deadline test.
- Devcontainer distribution/lifecycle checks: 49 tests passed. Original 12 project feature sets and mounts are preserved; all 24 original lifecycle bodies are preserved byte-for-byte after removing the added native setup block.
- Existing devflow identity tests: 42 passed. Git hook/setup tests: 37 passed. Native packaged policy smoke test passed. Repository TypeScript typecheck and diff whitespace checks passed.
- Every runtime bundle matches its SHA256 manifest. Existing Git indexes in both consumer repositories were preserved.

Tests cover real temporary bare pushes and intermediate commits, multiple live push URLs, immutable supplied OIDs after ref movement, raw Author/Committer despite mailmap, signatures/encoding/duplicate headers, credit aliases and AI allowances, entire-message limits, no-verify/optional option parsing/config overrides/autocorrect, editor cleanup, worktrees, hook stdin/chaining, guarded signing verification, safe reword with backup/tree/topology preservation, host migration and container-mounted host config isolation, symlink protection, permissions, idempotence and rollback.

## Independent reviews

Claude Code MCP completed a read-only migration plan review and an actual-source review. Its optional `-u` argument finding was reproduced with native Git, fixed, tested and re-reviewed; focused follow-up found no P2+ issue. A separate native worker reviewed the other implementation modules, identified unbounded pipe joins, and re-reviewed the deadline fix. The mounted-host configuration fix also received an independent read-only native review and a precise regression test. The final reword behavior review found no P2+ issue; the requested repair regression subsequently passed, and the declared-range limitation was documented.

LocalGPT GPT 6 Pro was requested for a conceptual plan review. Its response outcome remains unknown/unresponsive; no completed LocalGPT review is claimed and the request was not resent.

## Delivery boundary

The initial implementation phase performed no source commits or remote pushes. The clone-free distribution follow-up adds an explicitly requested initial source publication and v0.1.0 release. No live native installation was run on this Mac; the existing installed Python guard remains in place while the separate history-cleanup task is active. Its `guard.py` checksum remains `d01c526d5af9f3d9d1fceaaf3b56aaecbc8d873478c7e38397b8321b8fc03ece`. Native rollout is an explicit one-run setup after that task finishes. Windows binaries are not supplied. Direct alteration of binaries/configuration, absolute unwrapped Git, APIs and internal plumbing are outside this local access boundary.

## Clone-free distribution follow-up

Added a release downloader, fixed archive validation and checksum checks, release packager and generated four-platform Homebrew Formula. Mac validation: 62 tests passed (13 unit and 49 integration), including eight bootstrap tests. The helper subprocess fixture remains the only intentional ignored entry. Claude Code MCP performed separate read-only plan and actual implementation reviews; version pinning and strict checksum-row findings were resolved. All installation tests use isolated homes and fake release assets. Linux ARM also passed all eight new bootstrap tests. Published asset/download validation is performed separately.
