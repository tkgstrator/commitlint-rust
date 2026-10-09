# Commitguard Dev Container Feature

Installs the published Commitguard `v0.2.0` guard-only release plus the standalone
`commitlint-rust` message linter (`commitlint-only` release archive), and activates
guarded Git for the workspace repository when the container is created and each time it starts.

```json
"features": { "ghcr.io/tkgstrator/commitguard/commitguard:1": {} }
```

The original URI `ghcr.io/tkgstrator/commitlint-rust/commitguard:1` is still published for
compatibility; new configurations should use the URI above.

## Commands

- `commitguard` and its `gh-commit-guard` alias: the guard.
- `commitlint-rust`: standalone message linter; works on stdin/files without gh login or activation. No `commitlint` alias is installed, to avoid colliding with the npm command.

## Requirements

- Debian or Ubuntu image, amd64 or arm64.
- Depends on the official `git` (`os-provided`) and `github-cli` features.
- A **human** `github.com` login in `gh` (`gh auth login -h github.com`) for actual commits and pushes. Activation itself can complete before login. No credential is used or stored at build time.
- A container-private `HOME` and symlink-free managed agent/guard trees. Activation refuses symlinked or mounted managed
  paths (shell rc files, `~/.gitconfig`, guard state). Separate host mounts at
  `~/.codex` / `~/.claude` are allowed and never touched.
- `GIT_CONFIG_GLOBAL` must be unset or `$HOME/.gitconfig`.

## Options

| Option | Default | Meaning |
| --- | --- | --- |
| `autoActivate` | `true` | Run activation in `onCreateCommand` and `postStartCommand`. |

The option is recorded root-owned in `/usr/local/share/commitguard/options`;
runtime environment variables do not override it. Container administrators can modify image files.

## Behaviour

- Build: downloads `commitguard-<target>.tar.gz` and `commitlint-only-<target>.tar.gz` from the public `v0.2.0`
  release over HTTPS, checks embedded SHA-256 digests, verifies and extracts both before replacing commands or guard state, requires exactly
  `LICENSE`, `README.md` and the binary (`commitguard` / `commitlint`) as regular files, then installs
  `/usr/local/bin/commitlint-rust`,
  `/usr/local/bin/commitguard` and the `gh-commit-guard` symlink.
- Start (`setup --auto`): verifies the environment, then `commitguard install --container --repo <workspace>` with
  `CODEX_HOME`/`CLAUDE_CONFIG_DIR` inside the workspace. A ready marker is
  written only after the installed config, version and a guarded Git call
  succeed.
- A static shim at `/usr/local/share/commitguard/bin/git` (first on `PATH`):
  - `autoActivate: true`: ordinary Git **fails closed** until activation
    succeeded, printing login/setup instructions.
  - `autoActivate: false`: runs the native Git unguarded until
    `/usr/local/share/commitguard/setup` is run manually.
  - After activation: execs the installed guarded Git.
- If there is no repository or an unsafe layout, setup fails and Git stays blocked. After successful activation, unauthenticated, bot and incorrect main identities cannot commit or push. Authenticate using `gh auth login -h github.com`. Retry failed setup with `commitguard-devcontainer-setup`.

## Limitations

Commitguard v0.2.0 verifies the account through the GitHub API on every check
(no cache, no `--strict` flag, no bulk-write features of later releases). The published v0.2.0 linter supports a limited rule set, narrower than the 38 rules of current `master` source. The
shim only covers shells whose `PATH` includes the feature directory; absolute
`/usr/bin/git` bypasses it. Activation is per start and per workspace
repository found from the start directory.

Activation preserves existing hooks and adds the managed policy block to workspace `AGENTS.md` and `CLAUDE.md`. It creates native skills in workspace `.codex/skills/gh-commit-identity` and `.claude/skills/gh-commit-identity`; exclude these generated binary directories from source control. Shell configuration and global Git hooks are installed in the container user's private HOME. Native v0.2.0 also sets workspace-local devflow policy keys; those paths refer to this container.

The workspace must already be a Git repository. Initialize it before opening the container. A ready marker records the last successful installation; Git remains guarded or blocked if a later startup fails; the ready marker is only replaced after successful verification.
