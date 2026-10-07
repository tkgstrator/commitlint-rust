# Implementation plan

1. Add independent native bulk types/validation/state/writer, dispatch as bulk-write.
   Keep existing fix Source and replay unchanged. Reuse pure Rust message/raw
   identity policy and credential context; Git batches supply authoritative hashes.
2. Add meaningful RED acceptance tests with exact fixture gh/API/hook/process counts.
3. Implement private byte snapshots, batch source/hash/readback, ordered graph and
   date checks, explicit ownership/credit/header permissions and gitlink-only proof.
4. Add digest confirmation and compact durable phase records/OS-lifetime locks.
5. Native Mac/Linux targeted tests and 7,000/80 benchmark; inspect actual results.
6. Independent Claude diff review, resolve findings and ordinary guard regressions.
7. Document caller integration and coordinate checkpoint with Commit Fix. Do not
   replace live binaries or mutate its refs/journals from this implementation task.

LocalGPT is currently browser-busy and its prior Mac file access was unavailable.
Claude Opus read-only plan review completed; Codex resolved its dependency and
scope concerns by using Git dry hash authority and retaining object-only helper
responsibility. The existing operator owns promotion, backup/CAS and publication.
