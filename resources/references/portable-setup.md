# Portable native checks

Use `scripts/check` from this skill directory; it selects a bundled platform binary. Git and gh must be available on PATH. No package installation or language runtime is required. Missing binary, Git, gh, authentication, or unreadable history stops the operation.

Run `scripts/check account`, set all four Author/Committer environment fields from that fresh response, then run `identity`, `message <file>`, and `commits <oid>`. Run `push <remote> <source-ref>...` against every actual destination immediately before an already-authorized push. The hook uses `pre-push <actual-url>` and the exact Git stdin OIDs.

The message policy implements the supported Commitlint conventional rules, validated against a pinned development-only upstream oracle. The additional ASCII, entire-message 128-character and gh identity rules are guard policy. Repository config cannot weaken the guard. Arbitrary JavaScript configs/plugins and every upstream Commitlint rule are not supported.

Default conventional types: build, chore, ci, docs, feat, fix, perf, refactor, revert, style, test. Optional scope and breaking `!` are supported. Subject case, empty subject/type, full stop and trimmed-header rules retain Commitlint behavior. Blank-line warnings do not become failures. ASCII is a mechanical character check, not an English grammar detector.
