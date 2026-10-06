# One-run native setup

On a supported macOS/Linux host with Git and gh, run `scripts/check install` from this skill directory, or `sh scripts/setup-commit-policy.sh` from the source checkout. Setup installs native portable skills into CODEX_HOME/skills and CLAUDE_CONFIG_DIR/skills, preserves unrelated agent instructions and hook chaining, and explicitly activates the user-wide guarded Git entry point. Open a new shell afterward. Only creating the source or loading the plugin does not activate global policy.

`--home <directory>` supports isolated setup; use isolated GIT_CONFIG_GLOBAL/CODEX_HOME/CLAUDE_CONFIG_DIR as appropriate. `--skills-only` installs native agent skills without global Git/shell activation. Setup never changes Author/Committer settings or copies tokens. Fresh human github.com gh authentication is required when committing or pushing.

In a copied template, run `.devcontainer/commit-policy/scripts/check install --container --repo "$PWD"`. Container setup writes workspace-local skills and mandates, and keeps guard/config/shell integration in the container user's own directories. Container CODEX_HOME must resolve inside the workspace. Protected host-mounted ~/.claude and ~/.codex realpaths are refused, including symlink escapes. Use separate workspace agent directories if environments share a checkout concurrently. Authenticate using the writable environment-local GH_CONFIG_DIR or an explicitly provided caller token.

Existing hooks and verification programs are retained. New cryptographic signing is disabled. The PATH proxy forces the guard even for ordinary --no-verify/hooksPath attempts. Absolute unwrapped Git, APIs, direct config tampering and internal plumbing are outside this local access boundary. Existing active history repairs must finish before changing their installed guard binaries/configuration. Installation is reversible with managed backups and restores previous files on failed activation.

Consumers need no Rust toolchain, Bun, Python or node_modules. Build tooling and upstream Commitlint are development-only. Supported release targets are listed in the source README; an unsupported or missing target fails closed.
