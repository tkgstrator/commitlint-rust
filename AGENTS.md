# Commit policy implementation

Before any commit, history rewrite or push, read resources/SKILL.md and use scripts/check. Use the credential-bound verified human github.com gh identity for both raw Author and Committer; English printable ASCII Conventional Commits, at most 128 counted characters excluding validated final Co-Authored-By lines and their otherwise empty separator. Original ASCII and attribution checks still cover the whole message; other trailers remain counted. Normal checks are API-free; initialize with --strict account when required. Push and fix apply always verify online without fallback. For ordinary commits and V1 fix keep hooks active; explicitly authorized bulk-write may skip per-commit hooks/API. Verify actual commits and exact outgoing refs. Safe repair follows the skill's ownership/publication/backup/tree-preservation limits. Never bypass checks or automatically force push.

These identity/message requirements apply to newly created or rewritten local commits. GitHub-generated published merge/squash/rebase history has standing authorization for preservation: establish exact OIDs and origin from live github.com refs and merged-PR metadata, retain the original identities/signatures/messages, and automatically skip agent correction and approval requests for those differences. Names, emails, subjects and parent counts alone do not prove origin. Do not repair this inherited history or run standalone commits checks to trigger repair; use the mandatory native push/pre-push checker on actual outgoing commits. This exception grants no rewrite, force-push, ownership or migration authority and never bypasses a failed native check.

This repository is the native source. Consumer runtime requires Git and gh only. Cargo/Rust and the pinned Commitlint oracle are development tools. Preserve supported Commitlint rule compatibility through differential fixtures. Unsupported configuration/rules must fail explicitly rather than being silently ignored. Keep the public devflow legacy default unless strict mode is explicitly installed.

Native `fix` plans bind source OIDs, metadata and ownership separately from LLM-editable messages. Ordinary automatic repair retains the proven-own unpublished boundary. Explicit Author migration requires a current human instruction covering exact source OIDs/old Authors and their own mistakes, frozen per-OID declarations and current preview digest. Never infer this authority or bypass the existing source rebase gate; migration uses explicitly authorized hooked porcelain reconstruction. Verify actual trees/count/parents/dates/messages/identities, retained backups and complete OID mapping. Never implicitly push or force-delete dirty staging evidence.

Do not change the active host installation while its files are bound to an ongoing history-repair task. Test installation only with isolated HOME/GIT_CONFIG_GLOBAL/CODEX_HOME/CLAUDE_CONFIG_DIR, and never copy or log tokens.

Explicitly approved fast bulk rewriting uses bulk-write with frozen raw candidates and digest confirmation. It creates objects only: the operator retains backups, complete graph/tree/credit checks, expected-old promotion and separately authorized publication. Normal commits have no global ignore switch.

<!-- non-code-verification -->
## Verification for non-code changes

- For changes limited to audited development documentation, do not start heavy
  local or CI builds, simulator/UI suites, integration tests, or benchmarks.
  Run the lightweight scope/policy checks and record a documentation-only
  exemption; do not report unexecuted tests as passing.
- Source, tests, runtime/build inputs, assets, dependencies, lockfiles, signing,
  and CI configuration changes still require the applicable verification.
  Determine the actual changed inputs; an unknown or incomplete diff runs heavy.
- Use the repository's explicit documentation allowlist and adopted change-scope
  workflow. Markdown consumed by the product or tests is an input, not a blanket
  exemption. Keep required status names, lightweight checks, and existing
  manual, scheduled, release, and deployment gates intact.
- See `docs/development/ci-change-scope.md` for the common classifier contract.
<!-- /non-code-verification -->
