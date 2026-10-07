---
name: gh-commit-identity
description: Apply the explicitly opted-in gh-only strict personal commit policy before Git commit/rewrite/push operations and policy failures. Use for requested setup; never install or impose this policy merely because a plugin is loaded.
---

# Portable Git commit policy

For environments where the user has opted into this personal strict policy, apply it even without local Git hooks. Installing or activating this policy requires explicit setup authorization. Resolve this skill's actual directory from the skill catalog; do not assume a Mac path or an existing guard installation.

Every new commit requires:
- English printable ASCII text plus LF newlines, in Conventional Commits format checked by the native implementation of the supported Commitlint rules.
- At most **128 characters for the entire message**, including type/scope, body and trailers, excluding final LF terminators. ASCII is a mechanical limit, not a language/grammar classifier.
- Both raw Author and Committer equal the **verified human github.com gh account** (`type: User`): name `login`, email `id+login@users.noreply.github.com`.
- No local cryptographic signature. Main identities never come from a guessed name, OS user, Git config, an unbound cache, mailmap or AI provider.

Use the bundled native Rust checker. No commit or push proceeds after a nonzero exit, missing tool/dependency, failed authentication, uncertain ref selection or unreadable history. Never substitute an unchecked ad hoc validator, disable it, or use an API/other worker to escape it.

## Environment and tools

For authorized one-run host/container setup, read [references/host-setup.md](references/host-setup.md). Plugin loading alone never authorizes installation or global policy activation.

Read [references/portable-setup.md](references/portable-setup.md) on first use in a new environment. Git, authenticated gh and the supplied binary for this platform are prerequisites. Rust/Cargo, Bun and Python are not runtime requirements. Install missing prerequisites only within the user's setup authorization; otherwise report what is needed and stop.

If the known local Mac guard is installed, use its guarded Git entry point and preserve all hooks. Its absence on another environment is expected: use that environment's native Git plus this portable checker. Do not intentionally remove or avoid an installed guard. The portable checker itself finds Git and gh on PATH and runs the fixed Commitlint-compatible policy compiled into the binary.

## Before creating a commit

1. Run `<skill-dir>/scripts/check account` in the intended worktree. It returns the canonical `login` and `email` from the credential-bound verified cache. If unavailable or changed, run `<skill-dir>/scripts/check --strict account`; never invent a cache or silently weaken validation. Keep normal gh token precedence; never print tokens or enable HTTP debug logs.
2. Set **all four** `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME`, `GIT_COMMITTER_EMAIL` for the checking/commit command to those values, using the target shell's native command-scoped syntax. Do not modify global/repository identity settings unless separately authorized.
3. Run `<skill-dir>/scripts/check identity`. Write the proposed message to a UTF-8 file with LF endings and run `<skill-dir>/scripts/check message <file>`. If wording fails, automatically rewrite it from the intended diff into meaningful English such as `fix(auth): validate session expiry`, then check again. Do not truncate or replace it with a generic message that loses the change's meaning.
4. Commit through native/guarded Git with hooks active and signing disabled for that command. Then run `<skill-dir>/scripts/check commits <new-oid>` on each actual created commit. Verify the tree/diff and record the original OID; a worker's receipt is not proof.

Recognized AI `Co-authored-by`/`Signed-off-by` pairs, and exact legacy `Codex`, remain permitted by the current attribution policy in the native checker. Other human trailers must name the authenticated account. They count toward 128 characters and never replace either main identity. Do not invent attribution addresses or silently remove meaningful human authorship to pass.

## Before pushing

Resolve every destination and source ref in the **already authorized** push. Run `<skill-dir>/scripts/check push <remote-or-url> <source-ref>...` immediately before it. The checker uses live refs at all actual push URLs, checks every outgoing commit, and excludes only history already visible there; it ignores stale tracking refs and repository commitlint overrides.

For multiple remotes, check each. Expand `--all`, `--tags`, `--follow-tags`, mirror/refspec configuration and aliases into explicit sources/destinations before checking; if that cannot be done reliably, stop. Don't validate only HEAD and then push additional refs. Record the checked OIDs and confirm sources have not changed before the normal push. Do not automatically force-push.

Normal identity checks make no API calls. Explicit `--strict`, push/pre-push and native `fix --apply` validate online without fallback. A valid local cache cannot detect server-side revocation until a strict check.

## Automatic correction after a policy failure

For a candidate message not yet committed, rewrite and recheck automatically. For existing commits, make **one bounded repair pass** only when all of these are established:
- The task records prove these are the user's own newly created, unpublished commits; no live advertised ref at any relevant fetch/push remote contains them. No missing remote object or publication uncertainty is acceptable.
- The worktree/index are clean, no Git operation is in progress, and the affected suffix is linear. Authorship is canonical, or the task's creation records conclusively prove a mistaken identity on the user's own commit. Matching a name alone does not prove ownership.
- The user has authorized the original commit/push task. This personal skill records the owner's approved default to automatically correct compliant wording/metadata for this safe subset without asking again. Current user instructions override that default. It never authorizes a new push destination, force push or claiming another person's work.

Create and record a recoverable backup ref before rewriting. Inspect each original diff, prepare and lint all corrected messages, then amend/reword through ordinary Git with hooks active and the canonical command-scoped identities. Preserve every commit tree, commit count and parent topology. Do not squash, change files, bypass a blocking hook, or use plumbing/API commits as a workaround. If the existing guard blocks root/multiple-identity rebase, stop and report the blocked operation instead of weakening it.

The pre-rebase gate checks source authors and attribution in the declared upstream-to-branch range, permitting malformed source wording to be reworded. Edited todo lists or fork-point behavior can select additional objects; every resulting commit still needs independent validation. Sequencer picks may preserve invalid local messages without commit-msg; every actual resulting object must therefore pass the native checker before correction is reported or pushed.

After rewriting, compare each original/new tree and the old/new tip diff, check every repaired commit with `commits`, and rerun `push` preflight against fresh refs. Retry the same previously authorized normal push **once**. If anything remains invalid, stop with the violated rules and backup ref. Authentication, dependency, network and transport failures are setup/errors, not permission to rewrite. If a real push's outcome is uncertain or partially succeeded, inspect live remote state before any further action.

Published, foreign, merge-heavy or ambiguous history needs an explicit separate plan. No repair is performed merely because the checker failed.

## Receipt-based batch correction

When the installed native command supports `fix`, prepare an explicit range with
`commitguard fix --range BASE..HEAD --plan ../fixes.json`. Keep candidate and
ownership files outside worktrees or under the common Git directory. Edit only
candidate messages from their actual source diffs, retaining all source OIDs,
plan ID and recognized attribution. Do not infer wording by truncation or make
an ownership assertion from a matching display name. Preview with
`commitguard fix --preview ../fixes.json`; fix all findings before apply.
Ordinary apply uses `commitguard fix --apply ../fixes.json`; the proven own
unpublished safe subset above still defines permission for automatic agent use.

Author migration is a separate explicit operation, not an automatic fallback
when the existing rebase gate rejects a source. Require a current owner
instruction identifying the exact original OIDs/Authors as their own mistakes,
and freeze each declared source's exact raw Author value including date in an
ownership file. Only then use `--author gh --ownership ../owned.json` when
planning. Preview and apply with `--confirm-author-migration <apply-digest>`;
the digest acknowledges the displayed candidate content, not proof of a human
instruction. An LLM or delegated agent cannot create its own migration authority.
Every mismatched source Author must be declared; new Author/Committer must be the
fresh gh identity. Published/foreign/uncertain history remains outside this mode.

The native coordinator uses isolated hooked porcelain commits for that explicit
migration and ordinary rewording for default repair, preserves trees/count/parent
topology/Author dates and meaningful attribution, and validates actual result
objects before promotion. It never weakens source rebase checks or creates
plumbing/API commits. Record the returned backup, journal and OID mapping;
independently verify them and the actual commits before reporting success.
If interrupted or cleanup fails, inspect actual refs/state and preserve evidence;
never automatically reset, remove a stale lock or force-delete dirty staging.
Keep violating backup refs out of mirror/all-ref publication and still run push
preflight for every authorized source. New source code does not authorize
upgrading a guard bound to another active history-repair task.

## Deployment boundary

This is Codex workflow enforcement, not an immutable OS/Git server boundary. Another environment must install this skill and the supplied AGENTS mandate. Do not claim it is already installed or synchronized elsewhere. Local hooks remain a useful second layer. Pass this policy and the correct environment paths to delegated workers, and independently inspect their actual commits before pushing.

## Explicit fast bulk history rewrite

When the owner explicitly requests bulk rewriting with per-commit hooks and
GitHub authentication skipped, use the native `bulk-write` route. This scoped
exception supersedes hooked porcelain only for that approved existing-history
batch; ordinary new commits and V1 fix retain their guards. No global ignore or
arbitrary unchecked commit creation is authorized.

The current instruction and existing provenance must establish the exact source
scope and mistaken identities, including any already published history. Never
infer ownership or make declarations for foreign work. Use complete raw approved
candidates, exact source/candidate hashes, frozen ownership/credit/header changes
and expected OIDs. Preview, bind the returned digest, then confirm the same batch.
Initialize missing authentication explicitly with `--strict account`; warm-cache
bulk processing makes no API requests. Explicit `--strict` checks online once.

The native writer checks all objects together and creates a verified mapping;
it neither promotes original refs nor pushes. Preserve original backup refs and
operator journals, compare complete graph/count/ordered parents/both date fields,
trees and attribution, then promote with exact expected-old transactions. Only
explicitly approved path-specific gitlink changes may alter trees. Publication
still needs its own already authorized destination/ref scope and online preflight.
Coordinate any policy/binary upgrade with active tasks before changing their
bound files. Never claim another task's bulk replay or push completed from a
writer receipt alone.
