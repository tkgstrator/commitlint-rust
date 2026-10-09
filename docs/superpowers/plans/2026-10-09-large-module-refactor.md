# Large Module Refactor Implementation Plan

> **For agentic workers:** Use subagent-driven-development for independent module ownership. Do not commit or install; Codex integrates and verifies the combined changes.

**Goal:** Split the largest maintained Rust files by responsibility while preserving behavior and supported Commitlint compatibility.

**Architecture:** Keep existing module entrypoints and public exports. Move functions with their tests into private responsibility modules, using the narrowest necessary visibility. Extract installer phases through explicit context while retaining every command, write and rollback in the original order.

**Tech Stack:** Rust workspace; existing Cargo suites and frozen Commitlint differential oracle. No runtime dependencies added.

**Spec:** User request: consider splitting code files exceeding 400 lines. Assessment identified 14 maintained production files and 7 maintained integration test files; generated Unicode data and frozen upstream test sources are excluded.

## Constraints and review focus

- Preserve all public `rules::*` types/functions, all 38 rule names, UTF-16 semantics, defaults and exact diagnostics.
- Preserve installer preflight checks, protected host paths, permissions, transaction backup/rollback ordering and skills-only/container behavior.
- Preserve inspection ownership/publication checks, fingerprint bytes/order and entrypoint visibility. Binary hashes remain bound to receipts: upgrades intentionally invalidate old plans.
- Never update the active host guard; setup tests use isolated homes/configuration only.
- Prefer files at or below 400 lines; retain coherent larger functions/modules only with a recorded reason. Do not split generated or frozen upstream sources.
- Update the message-crate dependency boundary test to recurse into Rust module directories; retain its complete prohibition scan.
- Preserve `pub(super)` entrypoints through the facade using appropriately scoped re-exports; moving a function must not silently narrow or widen its caller access.
- Keep rule validation before dispatch, including on empty fields. Preserve installer errors currently inside the transaction and keep recheck/postcheck as distinct flows.
- Keep fingerprint shared-state validation interleaved with hash feeds exactly as before. Compare moved safety function bodies against the original source in addition to runtime tests.

## Tasks

- [x] Verify clean baseline with `cargo test --workspace --locked`; obtain independent native Claude plan review. LocalGPT plan review cannot start while disconnected. Baseline passed on studio.local; original test inventory retained for comparison. Claude review findings on recursive boundary scanning, visibility, command/error ordering and fingerprint inputs are adopted above.
- [x] Split `crates/commitlint-rust/src/rules.rs` into public type/registry facade, value validation, helpers, delimiter and rule-family evaluation modules. Keep public API paths stable. Split `tests/all_rules.rs` by topic within the existing integration target, retaining all cases and shared helpers. Run existing lint crate tests and frozen oracle checks.
- [x] Split `crates/commitguard/src/install.rs` into filesystem helpers, transaction, preflight/context and activation modules. Keep `install::run` orchestration concise and preserve ordered effects. Run existing installer unit and isolated integration tests.
- [x] Split `crates/commitguard/src/fix/inspect.rs` into path/source/repository/ownership/fingerprint/lifecycle modules. Keep orchestration readable; move relevant unit tests with their helpers. Run existing fix unit and integration tests.
- [x] Review remaining 400+ files and record whether a separate follow-up split offers a meaningful boundary; do not mechanically shuffle coherent code.
- [x] Obtain independent Claude review of the actual full diff, resolve material findings, run formatting, Clippy and the full workspace suite once after integration. Confirm test counts, public exports and unchanged safety algorithms against baseline.

## Completed validation

- `cargo check --workspace --all-targets --locked`: passed on studio.local.
- `cargo test --workspace --locked`: 297 passed, zero failed, three existing subprocess fixtures ignored in direct invocation.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- Node 26.8.2 / Bun 1.3.6 pinned upstream oracle: 15 tests passed after installing the frozen development lockfile.
- All 27 rule integration tests remain in the same target; frozen original corpus still checks 532 rule calls across 38 rules. Original guard, install and fix test inventories match after ignoring module prefixes.
- Native token comparisons preserve all 35 original inspection function/method bodies, 30 existing rule helper bodies, 22 extracted evaluation-arm bodies and all 27 moved rule tests. Comparisons retain string/character literals.
- Independent Claude Sonnet installer review and Claude Opus full-diff review found no regressions. Reviewers did not execute Cargo; Codex ran the checks above. LocalGPT remained disconnected, so its reviews could not start.
- Existing active host guard binaries (seven recorded paths) retain their original SHA-256 hashes.
- Final facade sizes: rules 130, install 29, inspection 17; largest new production module 341 lines, largest rule test module 277 lines.

## Assessment of other files

The first implementation targets the three largest production files and the large rule test suite. A separate read-only native inspection considered every other maintained file above the threshold. Keep this change focused on the approved priority files; the following boundaries are recorded for subsequent changes. The 400-line threshold prompts review rather than mechanical splitting.

| Production file | Decision after inspection |
| --- | --- |
| `fix/replay.rs` | Follow-up: separate private editor authority and bounded/redacted command diagnostics; retain replay and replacement verification together. |
| `bulk/tags.rs` | Follow-up: isolate receipt/object reproving and raw tag validation from preview/write coordination. |
| `bulk/validate.rs` | Follow-up: isolate attribution block consumption and binary tree/gitlink validation. |
| `fix/storage.rs` | Follow-up: separate canonical/duplicate-key JSON and durable journal persistence from private filesystem primitives. |
| `configured.rs` | Follow-up: move ordered JSON visitors/parsing; preserve configuration insertion order and avoid Cargo feature unification changes. |
| `core.rs` | Follow-up: isolate outgoing-ref/push validation behind existing `core::*` re-exports. |
| lint `git.rs` | Follow-up: isolate trailer subprocess capture, preserving its distinct deadline/process-group lifecycle. |
| `hooks.rs` | Optional signing/verification module; production is 386 lines and its policy flow is already cohesive. |
| `case.rs` | Retain: 400 production lines plus focused tests; scanner and Unicode transformations form one compatibility implementation. |
| `bulk/credits.rs` | Retain: 325 production lines plus grammar tests; splitting would scatter actor/count and grammar invariants. |
| `fix/apply.rs` | Retain: one durable operation lifecycle whose backup/staging/promotion/recovery ordering should remain visible together. |

| Remaining integration test | Decision after inspection |
| --- | --- |
| `bulk_runtime.rs` | Follow-up topical child modules for objects/approval/ownership/credits/origin, keeping the same integration target and fixture. |
| `fix_runtime.rs` | Follow-up topical plan/migration/hooks/publication/editor modules in the same target. |
| `install_runtime.rs` | Follow-up topical installation/container/wrapper/push/rebase modules; no duplicated fixture. |
| `bulk_tags_runtime.rs` | Follow-up metadata/signature/credit/receipt modules in the same target. |
| `native_policy.rs` | Retain this round: focused policy contract; origin-header cases are a possible later cohesive extraction. |
| `download_install.rs` | Retain: roughly half is its isolated fixture, and remaining cases form one bootstrap safety contract. |

Generated `unicode_properties.rs` and frozen upstream sources remain unchanged.
