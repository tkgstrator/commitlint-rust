# Implementation plan

Spec: ../specs/2026-10-09-all-commitlint-rules.md

1. Freeze public type/context/parser contracts and source/version provenance; preserve prior work. Pro plan review completed; Claude Opus review requested independently and still in progress. Apply known review conditions immediately; resolve further material findings before affected integration.
2. Oracle worker: build dev-only Bun transpilation + Node execution harness, execute verbatim TS tests with all actual matchers, parameterization and metadata layout; save assertion and outcome evidence separately. Verify all532 functional tests +3 metadata tests, mutation rejection and deterministic fixtures.
3. Case worker: generate Unicode17 property/case mapping data and oracle fixtures, implement normalization/word splitting/transforms/target aliases and subject gate with test-first Unicode/astral/context/ordinal/emoji cases. Avoid a general backtracking engine.
4. Engine worker: extend values/model/parser references and implement the remaining scalar/scope/breaking/raw-line rules; route all case rules through case module. Add bounded trailer Git stdin execution with fail-closed operational tests. Compare all original rule fixtures, preserving earlier APIs.
5. Coordinator: enable ordered JSON config, built-in presets/default ignores and configured severity evaluation; add explicit --config CLI path without the legacy fixed gate, while no-config/guard stay unchanged. Update compiled dependencies and narrow boundary tests; test positive/warning/disabled/invalid/unknown settings.
6. Integrate worker changes, fix actual differential discrepancies, update README/third-party notices and coverage evidence. Run native workspace and oracle tests, fmt/clippy, package contracts, deterministic regeneration and unchanged installed binary checks.
7. Request independent actual-diff Claude review and GPT6Pro review when available; verify findings against code/native output, fix material findings and reverify affected checks. Report exact covered profile and tests; do not imply arbitrary JS or live release updates.
