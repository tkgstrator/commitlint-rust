# All Commitlint v21.2.3 rules

The human requested all 38 exported upstream rules after the initial nine-rule port. Preserve the current fixed policy and prior uncommitted changes. This is rule compatibility within a documented native parser/JSON configuration profile, not arbitrary JavaScript/plugin compatibility.

## Required behavior

- Implement every exported name with rule-specific default condition/value, all standard typed values, exact Boolean and diagnostic text behavior, JavaScript UTF-16 lengths and delimiter semantics. Unknown rules, case targets, values and unsupported configuration fail explicitly.
- Preserve `lint_message` acceptance and error strings, no-config CLI behavior, and commitguard's ASCII/LF, whole-message128, identity and attribution policy. Configured lint has its own validity and never invokes the legacy acceptance gate.
- JSON opt-in accepts ordered rule tuples `[severity, when, value?]`, disabled `[0]`, built-in conventional/Angular parser presets and the conventional configuration preset (100-character limits), supported parser options, and default-ignore controls. Severity0 never executes; severity1 is nonblocking; execution/configuration errors are always fatal. Validate configuration before empty/ignored shortcuts intentionally. Unsupported JS, plugins, regex-based custom parsing or external extends fail explicitly.
- Preserve absent diagnostic message versus empty string. Preserve complete-tuple overrides and original rule order. Omitted values differ from null.
- Parser options include commentChar, issuePrefixes, issuePrefixesCaseSensitive and referenceActions. References participate in header/body/footer parsing; custom prefixes alter footer recognition. Maintain ordered reference records, action/owner/repository/issue/prefix/raw fields and upstream URL/dedup behavior. Keep raw text separate from comment/GPG/scissor-filtered parsing.
- Freeze case behavior to the actual Node26.8.2/Unicode17 and es-toolkit1.52.0 reference, including normalization/deburr, word ordering, ordinals, Emoji properties, case-insensitive subject gating and UTF-16 first-unit behavior. Existing regex uses Unicode16. Generated compact classification/case tables avoid compiler-dependent data. Compiled unicode-normalization0.1.25 is allowed. Bun1.3.6 uses Unicode15.1, so it only transpiles TypeScript; the original tests/rules execute under the pinned Node oracle.
- Only trailer-exists requires Git: bounded `interpret-trailers --parse` over original raw text. Explicit execution context controls Git path, cwd and per-call deadline/output limits. Production execution inherits caller Git environment/user/system/repository configuration, matching upstream; tests isolate these in their own child process. Missing/failing/timed-out Git, blocked stdin or held-open pipes are errors even for `never`/warning. No gh/network for message linting. Signed-off-by is native raw-line matching.
- Compiled serde_json (ordered objects), Unicode normalization, ECMAScript number formatting and semantic-version parsing do not add external runtime tools. Runtime remains Rust binary, with Git for Git operations/trailer rules and gh only for commitguard.

## Shared interfaces and ownership

- Engine worker: rules.rs typed values, all non-case rule dispatch, parser.rs/references.rs, bounded git stdin, scalar/scope/reference/trailer tests.
- Case worker: case.rs, generated Unicode properties/case mapping table, dedicated Unicode generation script and tests. API ensure_case(raw,target)->Result<bool>, is_target_case(target), subject_gate(char). Engine applies outer/per-case conditions and scope segmentation.
- Oracle worker: all upstream TS source copies, Node capture harness, fixtures and oracle tests, excluding the case worker's Unicode generator/table.
- Coordinator: Cargo/lock/lib module declarations, JSON configuration/configured lint/default ignore/CLI, documentation/licenses, final integration and verification.
- Public values retain None/Length/Text/List and add Number(f64), CaseChecks(Vec<CaseCheck>), ScopeEnum{scopes,delimiters}, ScopeCases{cases,delimiters}. CaseCheck has target:String and when:Option<RuleCondition>. Preserve float/negative/infinite/NaN direct rule semantics where representable; JSON only permits finite numbers. Format numeric messages using ECMAScript formatting.
- ParsedMessage gains references; ParserOptions and EvaluationContext are explicit. Keep operational Result errors distinct from rule violations.

## Acceptance

Execute all 532 original rule tests in 38 files, plus the three registry/documentation checks, with actual matchers and expanded parameterized callbacks. Record asserted expectations separately from observed rule outcomes, source/call/assertion locations and SHA256. Fail generation on unsupported matchers, failed assertions or unfinished async callbacks; no fabricated expected results. Add a mutated-expectation rejection test and deterministic regeneration.

Rust comparisons cover all38 name coverage, original outcomes/messages and parser/reference data, complex scope/case values, Unicode, numeric boundaries, aliases and operational Git failures. Config tests cover all severities, disabled trailer without Git, empty/ignored/config-validation boundaries, configured Unicode acceptance, preset limits/overrides/order and default fixed-policy preservation. Re-run existing221/86/5040 checks, workspace tests, format/clippy, packaging contracts and installed executable hashes.

No commits, history rewrites, push, live guard replacement or public release are part of this implementation. Retain the completed Pro plan review and await the independent Claude Opus review; resolve material findings before affected integration. Final independent Claude/LocalGPT reviews inspect the actual diff and target files.
