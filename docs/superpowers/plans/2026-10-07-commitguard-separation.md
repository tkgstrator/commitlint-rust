# Commitguard separation implementation plan

> **For agentic workers:** Use superpowers:executing-plans or subagent-driven-development to execute each task and verify its evidence.

**Goal:** Separate message linting from the Git/gh guard while preserving installed entry points.

**Architecture:** A Cargo workspace contains independent message and guard packages. Guard embeds the lint library; the canonical guard command and legacy command share one implementation.

**Tech Stack:** Rust, Cargo workspace, Git, gh, POSIX bootstrap.

**Spec:** ../specs/2026-10-07-commitguard-separation.md

## Global constraints

- Preserve the fixed printable ASCII/LF Conventional Commit subset and entire-message 128-character limit.
- Preserve fresh human github.com Author/Committer matching and recognized attribution rules.
- Runtime requires no Bun, Python, Cargo or Rust; no separate lint process for guard checks.
- No changes to installed host configuration, public releases or consumer checkouts.
- Keep old command names, guard root/config schema and all hooks/signature verification protections.

## Review focus

- A malformed guard config beside the standalone linter must not affect message-only lint.
- A release bootstrap must reject malformed archive members before extracting/executing.
- Reinstalling through the guarded PATH must resolve native Git and preserve hook chaining.
- New release assets must be fresh builds, never relabeled v0.1.0 payloads.
- Canonical and legacy names must run identical policy paths and install both usable names.

## Task 1: Independent message package

- [x] Add failing dependency-boundary and canonical-command tests before modifying production sources.
- [x] Extract `lint_message(bytes: &[u8]) -> Result<()>` into the message library; retain unchanged 221 oracle cases.
- [x] Move the standalone lint entry point to that package and remove all guard/core/config imports.
- [x] Keep Git range object reads bounded and `GIT_NO_REPLACE_OBJECTS=1`; stdin/explicit files never resolve tools or configuration.
- [x] Put attribution and identity checks in the guard package, calling the shared lint library.
- [x] Run independent message package tests and inspect `cargo tree -p commitlint-rust` for no guard dependency.

## Task 2: Guard package and compatibility

- [x] Move existing guard modules and integration suites to the guard package, adapting compile-time resource paths.
- [x] Share a single CLI dispatcher for `commitguard` and `gh-commit-guard`; version output identifies the invoked command.
- [x] Install both native executable names, preserving old generated checkers/hooks and config paths.
- [x] Add isolated install tests proving both names and continued migration/rollback/reinstall/bypass protections.
- [x] Run `cargo test --workspace --locked` and `cargo fmt --all -- --check`.

## Task 3: Release preparation and documentation

- [x] Set source package versions to 0.2.0 without modifying published 0.1.0 assets.
- [x] Update build scripts for the workspace and three executable outputs; refresh manifest from actual builds.
- [x] Keep bootstrap's exact four-member payload/member/type/count/hash validation, updating only the source release pin.
- [x] Package compatible combined, guard-only and message-only archives; split Formula dependency/install behavior.
- [x] Update bootstrap fixtures, docs, runtime dependency descriptions and version notes consistently.
- [x] Smoke-test release/archive installation with isolated HOME/config/agent paths and no compiler runtimes.
- [x] Request independent Claude actual-diff review; resolve material findings and rerun affected checks.
- [x] Report source/build completion and platform checks separately from publication or host rollout.
