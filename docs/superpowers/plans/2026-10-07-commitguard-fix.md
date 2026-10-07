# Commitguard Fix Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement task by task with test-first checks and an independent final review. The owner has instructed us to proceed with implementation.

**Goal:** Export, preview and apply batch commit message/identity corrections without weakening the installed guard or changing file trees.

**Architecture:** A new `fix` module owns immutable source receipts, editable candidates and durable operation state. Ordinary guarded Git constructs replacements in a detached worktree; independently verified replacements are promoted by expected-old-OID ref updates. External LLMs edit candidate messages only.

**Tech Stack:** Rust workspace, serde JSON, a Rust SHA256 library, Git and gh; no new runtime executables.

Use `sha2 = "=0.10.9"` only in commitguard; update Cargo.lock with native Cargo.
Codex uses `/opt/homebrew/bin/cargo` on studio.local. Claude's mounted source is
shared, but its Linux environment has no Cargo/gh; workers write tests/edits,
then Codex records native red/green runs. Do not install tools on a different host.

**Spec:** `docs/superpowers/specs/2026-10-07-commitguard-fix.md`

## Global Constraints

- Whole messages use the existing ASCII/LF Conventional Commit policy and 128-character maximum; recognized attribution is preserved.
- Both result identities match fresh human github.com gh; no signatures.
- Git >= 2.38 plus feature probes; Git and gh only at runtime (OS shell for hooks).
- Only clean, complete linear suffixes from an excluded existing base to current branch HEAD; all configured remote URLs and live advertised refs checked.
- Unknown remote ancestry, publication, ambiguous ownership, unsafe source objects, unsupported repository state or changed account/config/branch refuse apply.
- No automatic push, no real-history test rewrite, no host installation update. Fixture repositories and HOME/config are isolated.
- Only Codex creates actual development commits after native checker validation; workers must not commit/push.
- Initial fix storage/apply is Unix-only (Mac/Linux); fail explicitly on Windows.
- Private directories 0700, files 0600, exclusive create and O_NOFOLLOW; atomic journal rename and file/directory fsync.
- Proposal/ownership paths must be outside every worktree or under common Git-dir; reject source-root JSON output before writing (no dirty-file exemptions).

## Review Focus

- Plan edits expand the selected range or claim undeclared Authors: bind receipts and reject any mismatch.
- Hook chain resolves differently in staging: freeze absolute original hook context, verify executable digests, retain all hooks.
- Interruptions leave branch/journal disagreement: record intent durably and inspect actual refs on retry; no unconditional reset.
- Unknown remote tips are mistaken for unpublished ancestry: refuse even though ordinary push checker tolerates unknown tips.
- Ordinary Git creates a different message/tree/parent/date or drops empties: verify each actual object against the frozen plan before promotion.

## Task 1: Inspection, plan storage and preview

**Files:** Create `crates/commitguard/src/fix/{mod.rs,plan.rs,inspect.rs,storage.rs}` and `crates/commitguard/tests/fix_runtime.rs`; modify CLI/lib/Cargo as needed.

**Interfaces:** `fix::run(args: &[String], cfg: Option<&Config>) -> Result<()>`; serializable `Receipt`, `Candidate`, `Proposal`, `OwnedSource`, and `Preview` types with deny_unknown_fields and exact per-OID validation.

The existing CLI adds a fix arm after --config resolution. Export/preview allow
Option config; apply requires Some. Both executable names share dispatch; stem
git remains a guarded Git command and never invokes fix. Installer contracts stay.

- [ ] Write real-Git tests for unsupported CLI, exporting a two-commit suffix without mutating HEAD/index, preview of invalid/valid candidate wording, stale account/tip, duplicate/extra/missing candidate OIDs, source-owned declarations, and publication uncertainty. Assert actual refs/files and hand-derived messages, never source text.
- [ ] Run the new integration target before implementing; retain the missing-command/failure evidence.
- [ ] Implement strict CLI selection, raw source structural parsing, all remote URL discovery/live ancestry checks, history/worktree/config eligibility, exclusive bounded private receipt writes, canonical SHA256 receipt/apply digests and pure preview validation. Source receipt is authoritative; LLM JSON cannot change identity/range/provenance.
- [ ] Run the new integration target; all Task 1 tests pass and existing account/message/commit commands retain behavior.

## Task 2: Guarded replay and durable apply

**Files:** Create `crates/commitguard/src/fix/{apply.rs,replay.rs}`; extend Task 1 tests and isolated fixtures.

**Interfaces:** `apply::run(proposal_path: &Path, confirmation: Option<&str>, cfg: &Config) -> Result<ApplyResult>`; durable journal with phase, original tip, backup, staging path, verified OID mapping and digests. Private native editor helper dispatch belongs only to an active receipt/journal and never becomes a generic hook bypass.

Resolve the source's effective original hook directory using cfg repo_hooks,
worktree/common mapping and current hooksPath. Every staging/promotion call
passes guarded Git `-c core.hooksPath=<absolute-original-folder>`; wrapper strips
the caller's previous-hook env, so setting that env alone is insufficient.
Cleanup uses ordinary guarded worktree remove without force; retain dirty
hook-created evidence and record cleanup-incomplete. Tests cover annotated
remote tag objects and absent tag ancestry, not only lightweight tags.

Editors invoke the absolute current executable's private fix helper, bound to
receipt ID, active journal phase and exact todo/message path. Test missing
journal, incorrect ID/phase and unexpected source selections. This helper edits
only its operation's files and cannot change hooks or identity acceptance.
Migration restores from a clean previous mapped checkout using no-overlay
`git restore --source=<oid> --staged --worktree -- :/`; compare staged tree with
the frozen source via `git diff --cached --exit-code <oid> --` before committing.
Preserve deletions/modes/symlinks; refuse gitlinks/submodules in initial V1 if
their round-trip cannot be supported exactly. Never use commit plumbing.

- [ ] Add failing real-Git tests for multi-commit message/Committer correction, exact tree/count/parent/date preservation including empty commits, confirmed per-OID Author migration, wrong-Author default refusal and undeclared migration-source refusal.
- [ ] Run and retain failing evidence before replay code.
- [ ] Implement exclusive operation lock, atomic/checksummed/fsynced journal, backup ref and detached staging worktree with originating absolute hook chain. Reword via guarded interactive rebase with exact todo/native editors and controlled rebase settings. Explicit Author migration uses guarded restore of each frozen snapshot plus ordinary hooked allow-empty commits; canonical four identity variables and original Author date; no commit plumbing or unwrapped Git.
- [ ] Validate normalized proposed messages first and actual resulting commits afterward: exact tree, parent/count/date, identities, unsigned state, messages and attribution. Retain complete OID mapping before final fresh preflight and expected-old-OID branch promotion.
- [ ] Run Task 2 tests; original branch remains unchanged on every pre-promotion failure and successful tip has the original final tree.

## Task 3: Adversarial failure and recovery coverage

**Files:** Extend `fix_runtime.rs` and test helpers; correct production behavior only when a failing test demonstrates the bug.

- [ ] Add failing tests for original hook veto/message/index/ref mutation, relative hooksPath in linked worktrees, changed hook/config/account, concurrent tip/index/worktree changes, selected sources reachable from another ref/worktree/stash, published branches/tags and multiple push URLs.
- [ ] Add failing tests for malformed/encoding source headers, source signatures, replace/shallow/promisor state, unsupported Git/rebase config, stale locks, bounded/symlink/permission storage, interrupted journal/ref phases and repeated apply. Test cleanup and retained backup/mapping.
- [ ] Add exact-tree fixtures for deletions/modes/symlinks/gitlinks/filter conversions and empties. Unsupported round-trips must refuse promotion.
- [ ] Fix only demonstrated failures; run focused tests plus `cargo test --workspace` and `cargo fmt --all -- --check`. Keep compatibility and golden221 unchanged.

## Task 4: Documentation, independent review and delivery

**Files:** Update README, resources/SKILL.md and AGENTS.md for actual shipped behavior; record verified limitations explicitly.

- [ ] Document actual JSON schema, editing/preview/apply examples, migration ownership boundary, backup/recovery state and no implicit push. Synchronize source skill semantics without changing installed host policy.
- [ ] Request independent Claude review of actual diff/files and resolve material findings with reproducing tests; LocalGPT review is unavailable while browser_setup_timeout persists and must not be claimed.
- [ ] Codex runs focused and workspace checks natively, confirms only intended files changed and verifies integration. Compile target coverage only when available, report anything unexecuted.
- [ ] Commit through guarded Git with fresh checked gh identities and validated English Conventional message; validate actual commit. Do not publish/install a new release in this implementation task.

## Evidence

Native commands: `/opt/homebrew/bin/cargo test -p commitguard --test fix_runtime`,
`/opt/homebrew/bin/cargo test --workspace`,
`/opt/homebrew/bin/cargo fmt --all -- --check`.
Baseline on Mac: 78 passed, 2 ignored subprocess fixtures, no failures. Git is
2.54.0 (Apple Git-157). Golden221/oracle source remains unchanged.
Task completion and red/green logs will be recorded after actual execution.
