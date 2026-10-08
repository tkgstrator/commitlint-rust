# Approved immutable origin profile

## Authority and scope

Human approval is recorded at
`/Users/devonly/Developer/.identity-cleanup/resume-20261008/human-collision-header-approved-Rialto128-hold.json`.
It authorizes the source-sha256 collision profile for Izanami and Erudite Contents
legacy transformations. The Rialto 256-character exception was rejected: preserve
the entire-message 128-character limit and its existing held source/tag scope.
The operator must apply this profile only to those approved sources and retain
the exact frozen ownership decisions. A header never supplies ownership authority.

The completed feasibility review and explicit implementation approval cover this
bounded extension of the existing native batch writer. No installed guard, active
de8b executable, GitHub branch control or new publication scope is changed here.

## Interface and invariants

Keep schema version 1. Default policy version 1 remains byte-for-byte compatible
and refuses this header in bulk source, candidate and boundary objects.
An explicit policy version 2 manifest must carry
`"provenance_profile": "source-sha256-v1"`.
Each entry adding or preserving the header declares
`"source_provenance": "add"` or `"source_provenance": "preserve"`.
These declarations are forbidden in the default policy.
An undeclared v2 entry has the same header behavior as v1: neither its source nor
candidate may contain origin metadata. Explicit null, empty, unsupported profile
or mode values are rejected rather than treated as an implicit declaration.

For `add`, the actual source must have no origin header. Require a positive exact
source ownership declaration, even if the old Author is already canonical.
Repository/source scope is enforced by the coordinating operator against the
specific human approval. This reusable native backend does not hardcode repository
names or treat a caller-supplied remote URL as proof of that scope.
The candidate has exactly one final header immediately after canonical Committer:
`source-sha256 <64 lowercase hexadecimal characters>` followed by the existing
header/message separator. Its value equals the manifest source_sha256 after that
hash has been independently checked against complete actual original raw bytes.
It excludes the Git type/length envelope and includes original signatures.

For `preserve`, the source has one structurally valid origin header, the candidate
copies it byte-exact, and positive source ownership is explicit. Do not replace
the origin with the hash of intermediate rewritten bytes. Current raw source
OID/SHA256 and ownership are still independently checked.
For preservation, the operator must trace the copied marker to the retained
approved first-add receipt and original source snapshot; a syntactically valid
marker copied from an arbitrary intermediate object is not historical proof.
Header-bearing boundary
objects may be read only under this profile, without conferring ownership.
Source and boundary origin headers use the same strict final adjacent-to-Committer
slot as candidates. Newly generated profile objects already have that form;
legacy externally moved or folded origin headers are refused. Ordinary source
parsing order for other supported legacy fields is unchanged. Source committer
ownership rules remain independent and unchanged.
Neither adding an undeclared header nor removing, duplicating, folding, moving,
changing or inventing its value is accepted. All other unsupported headers remain
refused. Existing signature removal still requires exact declarations.

Messages, all required AI/human attribution and certification semantics, both date
fields, trees and ordered mapped parents retain existing checks. Do not change
passing message words to distinguish objects; no nonce, padding, dates, squashing
or credit deletion. Actual mappings must be injective, including the coordinating
operator's held-boundary and full-graph checks.

## Parsing and proof integration

Use a small shared Rust header validator for core actual-object/push checks and
bulk parsing. When present, known source-sha256 syntax is checked strictly;
ordinary unknown headers keep their existing behavior; the unknown-header refusal
is the bulk policy, not a new general push whitelist. A bare known key with no
space, noncanonical key case or whitespace delimiter, duplicate, uppercase/short
hex, trailing text, folding or wrong position
refuses; matching-looking message body text is not a header.
Ordinary identity/message/live-account rules remain. Ordinary push checks cannot
prove historical provenance from a digest alone: the operator additionally binds
actual outgoing CIDs/bytes to approved batch receipts and retained source evidence.

Bulk policy version 2 uses a separate digest domain. Bind profile/modes, original
and candidate bytes, current verified identity/context, and expected OIDs through
the existing immutable snapshots and intent/complete records. Existing v1 receipt
digests remain unchanged. Tag receipt reuse understands v2 and re-proves the same
domain, ownership, metadata, raw bytes and mapping rather than trusting JSON alone.

Future authorized rewrites of provenance-bearing history use explicit v2 preserve
mode. Ordinary V1 fix remains unchanged and fails closed on unsupported custom
headers; it must never silently discard this provenance. This limitation is
documented rather than expanding the hooked-porcelain repair interface here.
Ordinary Git commands are not claimed to preserve arbitrary extension headers
across all rewrites. In particular, independently approved origin-bearing rebase
or cherry-pick work must use the dedicated preserve pipeline and retain its source
evidence. This backend provides no general OS/Git-server immutability guarantee.

## Implementation and verification plan

1. Independent LocalGPT and Claude plan reviews of this spec and actual files;
   resolve material findings. Verify LocalGPT worker access before file review.
2. Add native runtime tests first and observe new-profile positive cases fail on
   current code while default rejection remains. Freeze tests before production edits.
3. Implement shared header syntax, manifest/profile validation, add/preserve rules,
   domain selection and tag receipt reproof with minimal existing-flow changes.
4. Run profile tests, ordinary guard/push tests, current bulk/tag tests, and full
   workspace checks on Mac and a disposable Linux container with --init.
5. Independent actual-diff review, resolve findings, verify final object bytes and
   injective maps in synthetic collisions, SHA1/SHA256 and interrupted retries.
6. Build a new immutable SHA-named backend, independently verify its hash, report
   manifest/push integration evidence to Commit Fix and coordinate use. Do not
   overwrite de8b or the installed guard; never implicitly promote refs or push.

Required cases include default/opt-in separation; first add, wrong digest,
malformed/duplicate/uppercase/folded/misplaced headers, declared ownership and
foreign refusal; source/candidate/body/tree/date/parent/credit preservation;
unchanged retry and changed-input refusal; later preservation and changed-origin
refusal; header-bearing boundaries; tag receipt reuse; normal actual-object and
push checks including malformed known headers; and zero per-object API/hooks.
Git fsck/roundtrip compatibility can be tested only in owned isolated fixtures,
never by writing original task repositories or weakening server controls.
Cover legacy v1 digest/receipt compatibility, cross-domain/tampered retained
profile refusal in tag reproof, and V1 fix refusal before creating repair state.
