# Immutable origin profile validation

The human approval covers Izanami and Erudite Contents legacy collision work.
Rialto's message-length exception was rejected and remains held. The operator
checks exact repository/source scope; the reusable backend does not hardcode it.

Final Mac arm64 backend SHA256:
`4c7441c07575f2d676caead59ea9f561c53419cd26627ff2d0c5d223c98b9d98`.
It is copied to a separate SHA-named mode-0555 executable. Installed guard/config
and the prior `de8b6b...` executable were unchanged during implementation/testing.
Runtime dependencies are still Git and gh; no Rust dependency was added.

## Evidence

- Mac full workspace: 208 passed, zero failed, two ignored subprocess fixtures
  exercised by parent tests.
- Linux full workspace: 207 passed, zero failed, the same two ignored fixtures.
  The owned disposable container ran with --init and was removed after completion.
- The unchanged v1 preview and complete digests and mappings were compared against
  the actual immutable de8b executable on identical isolated inputs: equal.
  Zero API calls/hooks during that compatibility run; original refs stayed unchanged.
- First-add collisions, SHA-1/SHA-256, signature-inclusive raw hashes, ordered
  merge boundaries, ownership, add/preserve/null/undeclared rejection, changed
  origins, known-header malformed variants and message limits are covered.
- A failed native write after actual immutable object creation retains intent,
  creates no complete receipt and changes no refs/index. Retrying the same approved
  batch succeeds exactly; changed input and a v1 confirmation digest refuse.
- Actual guarded test pushes reject malformed metadata without updating the remote,
  reject failed authentication, and accept canonical valid metadata. These are
  isolated local Git remotes, not production GitHub publication claims.
- Tag retargeting re-proves v2 receipts; retained policy/profile tampering refuses.
- Embedded mergetag continuation prose is preserved. Genuine folded origin
  metadata, tab delimiters and noncanonical key case refuse.
- Normal guarded amend was actually tested and retained its existing origin.
  No general retention guarantee is made for rebase/cherry-pick; later authorized
  origin-bearing rewrites use explicit bulk preserve and retained first-add evidence.
- V1 fix fails before creating repair state for origin-bearing source history.

## Review

Independent Claude plan review and Opus actual-diff review completed. The latter
found no blocking/high-severity defect. A continuation false positive was reproduced
and fixed; a separate bounded Sonnet follow-up found no remaining material issue.
Codex checked actual changes and ran the native tests; reviewers did not claim builds.

LocalGPT Instant verified its own studio.local LocalMCP file access in each review
session. GPT 6 Pro plan job `3a2db130-0b0a-482c-a168-30c724db5bcd` remained pending
with unknown/unresponsive outcome. Independent actual-diff Pro job
`619416e1-6b50-416f-bf6e-29461eabd969` failed with "Too many requests" and unconfirmed
native request. No automatic resend, browser workaround, or completed Pro review
is claimed. Useful conversations and job IDs are retained.

## Publication boundary

New core checks validate the known header's shape and normal identity/message
rules; they cannot attest historical provenance from a digest alone. The operator
must retain the approved first-add receipt/raw snapshot, validate outgoing actual
CIDs/bytes, perform backups/CAS and separately authorized online push checks.
Use policy version 2, provenance_profile source-sha256-v1 and explicit entry mode
add/preserve as documented in the approved spec. Never recompute an existing origin
from intermediate rewritten bytes or use this metadata as ownership authority.
The active guard is upgraded only at a coordinated safe boundary, using the same
reviewed backend; no protection, identity, credit or 128-character rule is relaxed.
