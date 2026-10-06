# Commit policy implementation

Before any commit, history rewrite or push, read resources/SKILL.md and use scripts/check. Use the freshly authenticated human github.com gh identity for both raw Author and Committer; English printable ASCII Conventional Commits, whole message at most 128 characters. Keep hooks active, verify actual commits and exact outgoing refs. Safe repair follows the skill's ownership/publication/backup/tree-preservation limits. Never bypass checks or automatically force push.

This repository is the native source. Consumer runtime requires Git and gh only. Cargo/Rust and the pinned Commitlint oracle are development tools. Preserve supported Commitlint rule compatibility through differential fixtures. Unsupported configuration/rules must fail explicitly rather than being silently ignored. Keep the public devflow legacy default unless strict mode is explicitly installed.

Do not change the active host installation while its files are bound to an ongoing history-repair task. Test installation only with isolated HOME/GIT_CONFIG_GLOBAL/CODEX_HOME/CLAUDE_CONFIG_DIR, and never copy or log tokens.
