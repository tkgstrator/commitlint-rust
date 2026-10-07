# Cached gh identity and strict online verification

The owner approved cached, API-free normal identity checks, explicit `--strict`
server checks, mandatory online push verification and online history-apply
verification. This supersedes the former fresh-user-API-on-every-check source
policy, without upgrading the currently installed guard during implementation.

## Identity authority

Never infer the numeric GitHub ID or owner of an arbitrary token from a login
label. Explicit strict verification calls github.com user, validates human User,
login and numeric ID, and derives canonical ID+login noreply email. Store only
those validated fields and credential/context fingerprints, never token bytes.
Ordinary checks compare current local gh credentials/context with that record.
A cold, malformed or changed record refuses with explicit strict-refresh guidance;
it never falls back to an API request. Strict failures never fall back to cache.
Offline checks do not assert current token validity or detect server-side rename
or revocation; online publication/apply gates remain mandatory.

Read the effective token through local `gh auth token --hostname github.com`,
captured privately in memory, and stored selected account through
`gh config get user --host github.com` (verified on the Mac). Respect GH_TOKEN
then GITHUB_TOKEN precedence; environment-only authentication may have no stored
account. Bind host, auth source, effective gh config directory, selected-account
context and a nonpublic SHA256 token fingerprint. Pin a strict API request to
the captured effective credential and reread local context afterward; a change
refuses verification/cache update. Disable gh update notifications, debug HTTP
logging and telemetry for local commands to maintain API-free ordinary checks.

## Cache and interfaces

Use private user storage separate from the installed guard's config/binaries,
with 0700 directories, 0600 files, exclusive/no-follow temporary creation,
atomic rename, fsync, regular-file and bounded-size checks. Cache schema records
hostname, context fingerprint, validated human account, numeric ID and verified
timestamp; validate canonical fields again when reading. Hash fingerprints are
not a trust boundary against same-user OS/filesystem tampering.

`core::account` uses cached mode unless a scoped strict mode is active;
`core::strict_account` always performs online verification. A scoped strict guard
is restored on exit and supports fix apply without weakening portable checks.
The two command names share CLI behavior. Support `commitguard --strict account`
and command-first strict guard options only where unambiguous. Never remove a
literal Git message/file argument named `--strict`. The Git proxy/creation hooks
normally use cached identity; explicit strict Git invocation may pass a
stronger-only environment flag to hooks. Push/pre-push always use strict account
even if no CLI flag is supplied. Fix apply must scope strict preflight and final
verification; replay hooks may use the already verified credential-bound cache.
Planning/preview may use cached identity but still need live Git publication
inspection to establish their separate unpublished-history boundary.

Explicit setup initializes cache only as part of successful authorized setup
or an explicit strict account command. No automatic host activation, token copy
or mutation of existing installed guard files is allowed. A plan made before
installation must be regenerated after config/executable context changes.

## Acceptance

Specific gh fixtures distinguish local config/token commands from API user and
record API calls. Test zero API calls for warmed normal account/message/commit
checks, explicit strict refresh, cold cache, changed account/token/configdir,
env-token precedence with no local account, malformed/private/symlink cache,
human/type/ID rejection, strict failure without cache fallback, cache races,
push forced online, and fixed-message/author policy parity. Existing isolated
fixtures initialize via the actual strict command, not fabricated cache JSON.
Native Mac and Linux runs plus independent source review are required. Lint
library, supported rules and golden221 remain unchanged.
