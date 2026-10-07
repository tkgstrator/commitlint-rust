# Commitguard authentication modes implementation plan

Owner approved implementation of the linked auth-modes spec. LocalGPT is
unavailable (browser_setup_timeout). Repository-only Claude review completed;
the earlier research review failed on unavailable WebFetch and made no edits.

**Spec:** `../specs/2026-10-07-commitguard-auth-modes.md`

## Tasks and interfaces

- [x] Add `tests/auth_runtime.rs` with exact local gh/API fixture and call counts.
  Native RED: 7 failures, unsupported strict command or unwanted cold-cache API.
- [ ] Add `auth.rs`: Mode Cached/Strict, `strict_scope()` RAII, credential-bound
  `account(&Tools, mode)` and API-free `context_fingerprint(&Tools)`. Store
  per-context records at XDG_STATE_HOME/commitguard or HOME/.local/state/commitguard.
  No TTL on ordinary cache; strict push/apply remains mandatory. No token bytes
  on argv, stdout/stderr, errors or disk. Pin strict API child GH_TOKEN to the
  locally captured credential; compare local context again before cache update.
- [ ] Preserve the old fresh human/API parser as `core::decode_account`; cached
  `core::account`, unconditional `core::strict_account`; force push/pre-push.
  CLI leading --strict never strips literal Git/message filenames. Stronger-only
  env propagation to creation hooks; fix apply scoped strict in parent.
- [ ] Add bounded capture with scoped secret env overrides and no telemetry/
  debug/update traffic. Private cache dirs0700/files0600/nofollow and atomic
  fsynced per-context refresh; validate canonical login/ID/email on reads.
- [ ] Specific shared fake gh supports local token/config/API, cache seeded via
  actual strict command. Installer verifies online and initializes cache only
  within successful authorized setup; preserve transaction rollback semantics.
- [ ] Root adds auth_context to private fix receipts/schema2 and recheck/postcheck
  binding; account-cache timestamp changes never invalidate receipts.
- [ ] Run native auth/fix/workspace suites on Mac and Linux; add strict cold,
  symlink/private-cache, env-only/preference, cache refresh race and literal-flag
  regressions as needed. Update source docs/skills to approved two modes.
- [ ] Independent actual-diff review, resolve material findings, native final
  evidence and guarded gh-only English Conventional development commit.

Code ownership: native auth worker owns auth/core/CLI/wrapper/util/install/lib and
auth/common fixtures; root owns fix modules, fix tests, docs and final integration.
Never change live guard binaries/config or actual repository history during tests.
