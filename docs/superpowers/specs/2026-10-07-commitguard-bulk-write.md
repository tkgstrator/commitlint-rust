# Native bulk history object writer

The owner explicitly requests skipping hooks and per-commit GitHub authentication
while rewriting existing history. This supersedes hooked porcelain for this
explicit bulk route only. Ordinary commits, V1 fix and push keep their guards.
The existing Commit Fix task owns approved messages, provenance, cross-repository
coordination, annotated tags, backups, ref promotion and authorized force-push.
Do not edit its active scripts or replace its installed guard without coordination.

## Interface

`commitguard [--strict] bulk-write --manifest FILE` previews and returns a digest.
Add `--confirm DIGEST` to write and verify objects. Both modes use one credential-
bound cached identity at entry; explicit strict authenticates online once. No API
or hooks run per object. The final local credential context must still match.
Push remains a separate online-verified action.

Manifest schema/policy 1 rejects unknown and duplicate fields. It binds the actual
canonical common Git directory and full source/candidate identifiers. Entries
contain source_oid, source_sha256, expected_oid, candidate_file,
candidate_sha256, optional exact old Author and Committer ownership, explicit credit changes,
explicit signature/header-removal permissions and optional gitlink substitutions.
A noncanonical source Author requires ownership:{old_author,owned:true}; a
noncanonical Committer requires committer_ownership:{old_committer,owned:true}.
These assert authority to migrate that proven-own commit metadata, not ownership
of a GitHub/web-flow/service account. Genuine foreign contributions remain held.
Human credit_changes to the same canonical-human role require owned:true for an
old noncanonical alias; human-to-AI and Signed-off-to-Coauthor conversions refuse.
main_credit_changes:[{role,old_identity,new}] freezes exact legacy AI main-role
provenance. Registered AI main roles must retain equivalent credit automatically;
new Signed-off certification is never inferred. Equivalent AI Coauthor->AI-credit
compact changes require explicit mappings; multiple contiguous old blocks may
be joined with LF in one old field and mapped to one counter-equivalent compact
block. Provider/model/version/context and actor multiplicity cannot change.
Boundary parent OIDs are declared and verified. Source and expected OIDs are
unique. Candidate files contain complete approved raw commit bytes.

Ownership and credit declarations record existing explicit human approval; they
cannot invent authority. Recognized AI credit must retain provider/model/version
and context. Any exact legacy-credit normalization is an explicit, frozen mapping.
Operator approval records and final graph verification remain with Commit Fix.

## Invariants

Read real source objects with replacements and lazy fetching disabled, batch all
reads and hash checks. Roots and ordered merge parents are supported. Candidates
must have the canonical gh main identities, printable ASCII English Conventional
Commits of at most 128 entire-message characters, both original date fields,
ordered parents mapped exactly from their actual sources, and the original tree.
Changed trees permit only declared path-specific mode-160000 old/new gitlink
substitutions; recursively prove unchanged names, modes and all other OIDs.
No arbitrary new blob/tree changes. Unsupported headers fail explicitly; removing
obsolete source signatures/encoding headers requires exact explicit permission.
Preserve recognized AI credit multiplicity and exact approved normalizations.

Snapshot manifest, source and candidate bytes into a private owned operation
folder under the common Git directory. Never pass caller paths directly to Git.
Hash source and candidate snapshots in one dry `hash-object -t commit --stdin-paths
--no-filters` call: Git must produce every original and expected OID exactly.
This avoids a Rust SHA-1 dependency and retains Git's authoritative hash checks.
With confirmation, one `-w` call writes all candidate snapshots. Read back actual
objects in one batch and compare complete bytes and policy. No commit hooks,
index, checkout, original ref, signing or network mutation is performed.

Durable compact intent/mapping records are written per phase, not per commit.
Snapshots are atomically published after fsync, with interrupted temporary copies
retained. The write pass forces loose-object fsync before complete evidence.
Keep evidence on interruption; identical object writes are idempotent. Operation
locks use OS lifetime semantics. Return only a verified mapping after postchecks.
The outer task must preserve backups and perform expected-old ref transactions
and final graph validation before publication; this command never pushes or
silently promotes refs. Ordinary guarded Git receives no global ignore option.

## Validation

Test source/candidate hash mismatch before writes, malformed manifests, private
snapshot paths/symlinks, roots, ordered merges, boundaries, dates, identity change,
foreign ownership, attribution loss, unsupported signatures/headers, tree/blob
changes, allowed gitlinks, duplicate/collapsed mappings, failed/truncated batch
commands, retry and exact readback. Cover SHA-1 and SHA-256 repositories.
Count processes/API/hooks and benchmark 7,000 objects across 80 isolated repos.
Warm-cache acceptance is zero API/hooks and Git process count by phases/repos,
not commits; report actual timings without promising a speedup beforehand.

## Minimal manifest shape

```json
{
  "schema_version": 1,
  "policy_version": 1,
  "common_dir": "/absolute/repository/.git",
  "boundaries": [],
  "entries": [{
    "source_oid": "<full source OID>",
    "source_sha256": "<SHA256 of complete original raw bytes>",
    "expected_oid": "<full candidate OID>",
    "candidate_file": "/absolute/approved-candidate.commit",
    "candidate_sha256": "<SHA256 of complete candidate raw bytes>",
    "ownership": null,
    "committer_ownership": null,
    "main_credit_changes": [],
    "credit_changes": [],
    "remove_headers": [],
    "gitlinks": []
  }]
}
```

Use actual identifiers and raw hashes, not the descriptive placeholders above.
Dates remain part of old_author/old_committer/old_identity declarations. Optional
fields may be omitted. Duplicate keys, unsupported headers and unused declarations
refuse; distinct source nodes cannot silently collapse to one expected OID.
The exact footer `🤖 Generated with [Claude Code](https://claude.com/claude-code)`
may be explicitly mapped to `AI-credit: Claude Code`. Other narrative/HTML/footer
normalization requires a separate provenance adapter; this helper never guesses
from prose. A single actor may carry a canonical positive count suffix `xN`
(1..100000), for example `AI-credit: Claude Opus 5.5 x141`. Counts must match the
actual source attribution multiplicity exactly.
For noncontiguous repeated credits, use one change with `source_blocks` containing
each exact physical source credit block (duplicates included) and one `new` compact
credit. Omit `old`; `old` and `source_blocks` are mutually exclusive. Every listed
block is consumed once from actual source credits; invented blocks or excess
repetitions refuse. This grouping is limited to recognized AI attribution.
Tags require contiguous `old` replacements to preserve their exact prose.

## Annotated tags

`bulk-write-tags --manifest FILE [--confirm DIGEST]` uses schema/policy version 1,
the same `common_dir`, optional `commit_receipt` (absolute completed native commit
receipt), and `entries` with source/candidate OIDs and SHA256 fields as above.
Each entry may declare `tagger_ownership: {old_tagger, owned: true}`,
`credit_changes`, `main_credit_changes` with role `tagger`, and
`remove_signature: true` for an actual terminal PGP/SSH source signature.
Tag candidates preserve names, types, original date fields and exact prose except
declared equivalent credit changes. They carry no signatures. Retargeting must
match the revalidated commit receipt or earlier mapped tags in the same batch.
Tag messages retain their existing prose and do not use commit-message limits.
Tag batches never modify refs, index or worktrees; the caller still owns backups,
ref transactions, final graph checks and publication.
Completed commit receipts retain their original approval identity and credential
fingerprint for digest reproof. A later tag batch may use a rotated credential for
the same currently verified human account: it revalidates the actual commit
objects and binds its own current credential context. Changing the retained
historical context or identity breaks the original receipt digest. New human
co-author or sign-off blocks cannot be invented during commit rewriting.
