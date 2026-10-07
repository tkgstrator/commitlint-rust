# Batch message repair and explicit identity migration

Status: design for owner review; no product implementation or host rollout.

## Intent and constraints

The owner wants batch correction of overlong/nonconforming commit messages and
incorrect Author/Committer metadata. Rust remains the runtime implementation;
Git and human github.com gh authentication are the only required tools. Planning
and preview use the verified credential-bound cache; apply always validates
online. Receipts schema/policy 2 bind the local authentication-context digest,
so replacing credentials invalidates approval even for the same account.
An external LLM or editor may prepare messages. No LLM service, Python, Bun,
git-filter-repo, or Cargo becomes a runtime prerequisite.

`commitlint` stays a message validator. `commitguard fix` owns enumeration,
repair plans, history changes, identity checks, backups and result validation.
An automatic hook must never silently rewrite arbitrary outgoing history.
Push remains an independent, explicitly requested operation.

## Two explicit operations

1. **Repair** fixes messages and canonicalizes Committer. Source Author must
   already be canonical and source attribution must satisfy existing policy.
   The existing pre-rebase gate remains unchanged. A CLI request to apply an
   exact plan is explicit repair intent; unattended agent repair additionally
   requires the creation/provenance evidence already mandated by the skill.
2. **Author migration** changes a specifically selected mistaken Author to the
   same verified gh identity. This is an explicit migration of the operator's own
   work, never inferred from an old display name, email, or an LLM response.
   It needs a per-source-OID ownership declaration reviewed by the operator,
   a separate migration plan, and explicit acknowledgement when applying it.
   This expands the skill's explicit migration workflow, not its automatic
   safe-repair permission. Foreign work cannot be claimed by automatic repair.

In a migration range, every noncanonical source Author must have an exact
OID/old-Author declaration; an undeclared mismatch anywhere in the suffix stops
the operation. The receipt freezes declarations including exact old raw Author
bytes. Changes to owned.json after planning have no authority.

Migration ownership declarations document a human assertion; they do not
cryptographically prove who originally wrote a change. If Codex cannot
establish ownership from task records, it must ask the owner to identify the
exact mistakes rather than generate those declarations itself. Both operations
produce canonical raw Author and Committer; arbitrary destination identities
remain outside this gh-only tool's policy.
The CLI acknowledgement is an intent check, not proof of a human instruction:
an agent may migrate Authors only under an explicit user instruction covering
the exact old OIDs/Authors presented for migration. The binary cannot verify
conversation or creation records; the skill enforces that boundary.

## CLI and editing contract

Proposed commands (not yet available):

```sh
commitguard fix --range origin/main..HEAD --plan ../fixes.json
# External editor or LLM changes candidate messages in ../fixes.json.
commitguard fix --apply ../fixes.json

# Separate, deliberate Author migration; owned.json enumerates exact own OIDs.
commitguard fix --range origin/main..HEAD --author gh --ownership ../owned.json --plan ../migration.json
commitguard fix --preview ../migration.json
commitguard fix --apply ../migration.json --confirm-author-migration <apply-digest>
```

`--plan` performs inspection and writes a proposal plus a repository-local
source receipt. It creates no commits and changes no branch/index/worktree.
Unknown or conflicting flags fail. Output files are created exclusively,
without overwriting or following symlinks. A no-change apply reports a no-op.
`--author` accepts only `gh`; other values fail. `--preview` validates without
creating commits or changing refs/index/worktree. It shows all source OIDs,
old/new Authors, lint findings and the final apply digest.
Keep proposal/ownership files outside all worktrees or under the common Git
directory in V1, so the operation does not dirty its own source checkout.
Refuse in-worktree output before creating any file; no general filename
exemptions permit untracked edits. Bind the proposal absolute path in receipt.

The native receipt freezes schema/policy version, operation, plan ID, common
and worktree Git directories, current branch, object format, original tip/base,
ordered full source OIDs, single parents, trees, raw metadata/message bytes,
recognized attribution, gh identity, ownership declarations, effective guard
binary/configuration and hook-chain fingerprints, and destinations used for
publication checks. Store it under the common Git directory with
private access and bounded file sizes. Read-only inspection data is separate
from the external candidate JSON; the editable JSON contains only plan ID,
source OID and proposed message. It cannot change range, ownership or identity.
Reject unknown fields, duplicates, omitted or extra commits and unsupported
schema versions; recheck all receipt values against actual state at apply.
The plan ID is SHA256 of a canonical source receipt excluding its own ID field;
the apply digest additionally binds all normalized candidate messages. Define
canonical serialization once and test it. After editing candidates, preview
again; migration confirmation must match the current digest. These hashes detect
stale/mismatched inputs, not hostile same-user filesystem tampering.
An active guard or hook-chain change invalidates the plan. Guarded Git and
config must be available for apply; planning may use the portable checker,
but must explain missing apply prerequisites instead of installing anything.

Do not silently shorten, translate, guess a type, invent a message, or delete
attribution. Export original wording and lint findings. The external author of
the proposal uses source diffs to write meaningful English. Diff export is
explicit; the binary does not upload files or call an LLM. Preserve recognized
AI and meaningful human attribution. All final messages, including trailers,
must pass the compiled ASCII/LF, Conventional Commit and whole-message 128
character rules. An impossible length/attribution combination is an error.
Validate after the same cleanup used by commit-msg, and require actual created
messages to equal approved normalized bytes.

## Initial history boundary

Both operations accept only a complete linear suffix from an existing base to
the current named branch's HEAD. The base is a strict ancestor of HEAD and is
not rewritten. Keep empty commits and the exact source count. Require clean
index/worktree, no sequencer/rebase/merge operation, no shallow/grafted/replaced
history, no unsupported partial/promisor repository, and readable raw objects.
Reject malformed or duplicate identity headers and unsupported commit headers.
Accept normal tree/parent/author/committer headers and well-formed multiline
source gpgsig/gpgsig-sha256 metadata; do not guess encoding or interpret opaque
extensions.
Author dates/timezones are retained; new Committer dates are generated for the
operation and recorded. New commits are unsigned, consistently with policy;
any original signatures stay recoverable through the backup.

Enumerate every configured remote's fetch and push URL, including multiple
push URLs, and check live advertised refs. A stale tracking ref is insufficient.
For both operations, every selected source must be absent from history
reachable from live advertised refs at every destination. Hidden server refs
remain outside this evidence. In the initial version, unknown advertised objects
cause a fail-closed refusal; existing outgoing
push logic that tolerates unknown tips cannot prove unpublished ownership.
Missing authentication, unresolved destinations, inaccessible remotes, or
publication uncertainty stops apply. Zero configured remotes is unresolved
publication evidence and refuses apply in this initial version. Recheck immediately before
promotion. The local tool cannot exclude publication to unknown external forks
or an unrelated actor publishing after the check; make this boundary explicit.

Root history, published commits, merges, tags, multiple-branch rewriting and
git-filter-repo integration are future extensions requiring a separate design.
There is no force-push or implicit push in this feature.

## Execution, isolation and hooks

Use an exclusive repository repair lock, durable journal and recoverable
`refs/commitguard/backups/<operation-id>` before creating replacement history.
Replay in a detached linked worktree; the repair coordinator leaves the original
branch/worktree at the old tip until all checks pass. Hooks execute arbitrary
user code and can affect shared refs or external state; detect relevant changes
and report them, without promising rollback of arbitrary hook side effects.
Bind the journal to receipt/normalized proposals,
original tip, target identity, backup, staging worktree and phase transitions.
Private files must reject unsafe permissions, symlinks and conflicting plans.
Write journals atomically with checksums, fsync files and their directory, and
record phase intent before mutations. Never automatically break a stale lock;
inspect/report its operation and actual refs first.
Existing original hooks and their effective paths must run in the staging
worktree too; do not lose a per-worktree hook mapping when moving to it.
Resolve relative original hook paths in their originating repository context;
reject an unresolvable chain. Linked-worktree config must not accidentally
change the original repository's shared configuration.
Freeze the absolute original hook directory and executable contents, passing
its context to every staging call through the guarded wrapper. Include hooks
fired by worktree creation and throwaway commits in the execution contract;
post-checkout, post-commit and post-rewrite are not suppressed.
Pass the frozen original directory with guarded Git's operation-local
`-c core.hooksPath=<absolute-original-directory>`; the wrapper forwards it as
previous-hook context while forcing guard hooks. An inherited previous-hooks
environment variable alone is deliberately discarded by the wrapper.

Repair uses ordinary guarded interactive rebase with native Rust sequence and
message editor helpers. Verify the todo against the complete source list; no
exec commands, autosquash, fork-point selection, root rebase, update-refs,
commit dropping or unchecked editor shell interpolation. Force recreation
where needed to normalize Committer and preserve empty commits. Test the exact
Git-version behavior and source-OID mapping before selecting command options.
Require Git >= 2.38 and tested feature probes; fail if required behavior is
unavailable. Set operation-local controls for autosquash/updateRefs,
instructionFormat/abbreviateCommands, missingCommitsCheck, templates, comment
character, cleanup and sequence/message editors. Use full OIDs in the verified
todo and map by position with independent parent/tree/date checks, not solely
post-rewrite output. Never inherit arbitrary editors or rebase exec settings.
Git 2.38 introduced update-refs; see the
[official release notes](https://github.com/git/git/blob/v2.38.0/Documentation/RelNotes/2.38.0.txt)
and [versioned rebase documentation](https://github.com/git/git/blob/v2.38.0/Documentation/git-rebase.txt).

Author migration cannot use that rebase path: the current source gate correctly
rejects noncanonical authors. Instead, its explicitly approved source receipt
permits reconstructing each original tree on the isolated base through ordinary
guarded porcelain commits with all four canonical identity variables and the
approved message. Concretely, restore the frozen snapshot with guarded
`git restore --source=<source-oid> --staged --worktree -- :/`, compare the staged
tree with the source, then use hooked `git commit --allow-empty -F <message-file>`.
Set all four canonical identity variables and the recorded original author
timestamp/timezone explicitly. Account for file deletions, modes, symlinks and
gitlinks; reject any snapshot that cannot be reproduced exactly.
Disable commit.gpgsign for each commit. Refuse a worktree round-trip altered by
filters/LFS, autocrlf/eol conversion or filesystem case collisions.
Staging an original snapshot is an intentional migration
operation, not a fallback when ordinary repair fails. All existing commit hooks
remain active. Do not use commit-tree, fast-import, raw object writes, an
unwrapped Git command or API commits to escape policy. No general hook bypass
or environment allowlist is introduced. Skill/docs must explicitly describe
the new migration boundary before this path is implemented or delegated.

For each actual new commit validate canonical identities, message/attribution,
unsigned state, expected tree, exact single parent and count. Compare every
source tree with its mapped result and compare original/new final trees.
Hooks may reject, edit messages/files or refs; any discrepancy stops promotion.
Record a complete original-to-new OID mapping and actual verified bytes.

Before promoting, refresh gh and live refs, and recheck the original branch,
worktree/index and any linked worktrees. Refuse shared-branch ambiguity or an
unexpected concurrent operation. Promote only with old-tip compare-and-swap
through hooked Git ref updates. The final tree is unchanged, so the original
index/worktree needs no destructive checkout/reset. Never overwrite a changed
branch. This lock coordinates commitguard operations, not arbitrary other Git
processes; CAS and repeated checks detect relevant conflicts, not OS isolation.
Another worktree attached to the branch, or another branch/tag or worktree HEAD
containing selected sources, is a conflict; reject it. Backup refs, operation
staging refs and reflogs are recoverability records, not destinations to update.
Exempt only this operation's recorded staging worktree HEAD from that conflict
rule; it initially points at the frozen original tip before replay.
Stash refs containing a selected source also stop apply. The residual
publication/index/ref race remains; there is no distributed transaction.

On failure retain backup/journal and report whether the original ref changed,
the verified partial result, and the exact recovery state. Never auto-reset or
delete unrelated work. A crash after ref promotion but before journal finalization
is detected from actual ref/object state. Repeating apply cannot duplicate or
silently overwrite an incomplete operation; it reports the existing operation
and recovery instructions. Successful cleanup removes only verified temporary
worktrees and keeps the backup/mapping.
Never force-remove a staging worktree modified by hooks. Retain it and record
cleanup-incomplete with its path if ordinary guarded worktree removal refuses.
Backups retain original violating commits. Broad --mirror/all-ref publication
must still validate them and fail; document the backup ref and avoid treating
it as a normal push source. Do not delete backups to make a push pass.

## Integration and acceptance

Add fix dispatcher/module in `crates/commitguard`; keep the compatibility binary
and existing config/hooks/installer contract. Lint rules and golden221 remain
unchanged. Add repair-specific replace/promisor detection instead of assuming
completeness of core::history_safety. Update AGENTS.md and resource skill
guidance for plan editing, explicit migration,
backups and postvalidation. Do not activate new host binaries or rewrite any
real repository as part of implementation tests. Release and consumer rollout
are separate completion steps with the active-history-repair coordination rule.

Use real Git, fixture gh and isolated HOME/config in tests. Required coverage:

- Invalid English/ASCII/Conventional/128-character messages and preserved
  attribution; duplicate/extra/changed plan entries and stale receipt/account.
- Multiple commits including empty commits; exact trees/count/base/parents,
  dates, unsigned results, canonical identities and complete OID mapping.
- Default wrong-Author refusal; explicit per-OID own-Author migration succeeds;
  no inferred ownership, foreign credits or arbitrary destination identities.
- Published/tag-reachable sources, every fetch/push URL, unknown objects,
  network/auth failures, shallow/replaced/promisor history and operation locks.
- Original hooks execute, can veto, and cannot silently change approved trees
  or messages; source branch changes, dirty source worktree and CAS conflicts.
- Failure/crash phases around backup, each commit and promotion; retry reports
  actual journal/ref state with no destructive automatic recovery.
- Runtime PATH with only Git/gh and platform hook shell; paths with spaces,
  linked worktrees, source and installed canonical/legacy entry points.
- Undeclared wrong Author anywhere in a migration range; frozen old Author and
  ownership declarations; changed proposals invalidate migration digest.
- Relative source hooksPath transferred correctly; filters/LFS/eol conversion,
  symlinks, deletions, gitlinks and case collisions fail or preserve exact trees.
- Git-version/feature minimum and hostile rebase/editor config; signed source
  support, unsupported encoding headers, stale locks and backup mirror refusal.

Independent review of actual code/diff and native evidence are required before
completion. Build verification covers supported release targets; explicitly
report compile-only targets and unexecuted environments.
