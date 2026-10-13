# Commitguard Dev Container Feature

Installs the published Commitguard `v0.3.0` guard-only release plus the standalone
`commitlint-rust` message linter (`commitlint-only` release archive), and activates
guarded Git for the workspace repository when the container is created and each time it starts.

```json
"features": { "ghcr.io/tkgstrator/commitguard:1": {} }
```

The long URI `ghcr.io/tkgstrator/commitguard/commitguard:1` remains the source for
the short URI above. The old `ghcr.io/tkgstrator/commitlint-rust` namespace is retired;
migrate configurations using `ghcr.io/tkgstrator/commitlint-rust/commitguard:1` to
`ghcr.io/tkgstrator/commitguard:1`. Use explicit version tags
(`:1`, `:1.2` or `:1.2.0`); the short package reserves `:latest` for collection metadata.

## Commands

- `commitguard` and its `gh-commit-guard` alias: the guard.
- `commitlint-rust`: standalone message linter; works on stdin/files without gh login or activation. No `commitlint` alias is installed, to avoid colliding with the npm command.

## Requirements

- Debian or Ubuntu image, amd64 or arm64.
- Depends on the official `git` (`os-provided`) and `github-cli` features.
- A **human** `github.com` login in `gh` (`gh auth login -h github.com`) before activation. Native installation verifies the account online and initializes a private credential-bound identity cache. No credential is used or stored at build time; the cache never stores tokens.
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

- Build: downloads `commitguard-<target>.tar.gz` and `commitlint-only-<target>.tar.gz` from the public `v0.3.0`
  release over HTTPS, checks embedded SHA-256 digests, verifies and extracts both before replacing commands or guard state, requires exactly
  `LICENSE`, `README.md` and the binary (`commitguard` / `commitlint`) as regular files, then installs
  `/usr/local/bin/commitlint-rust`,
  `/usr/local/bin/commitguard` and the `gh-commit-guard` symlink.
- Start (`setup --auto`): verifies the environment, then runs `commitguard install --container --repo <workspace>` with
  `CODEX_HOME`/`CLAUDE_CONFIG_DIR` inside the workspace. Installation requires
  strict online verification of the human account. A ready marker is written
  only after installation, config, version and a guarded Git call succeed.
- A static shim at `/usr/local/share/commitguard/bin/git` (first on `PATH`):
  - `autoActivate: true`: ordinary Git **fails closed** until activation
    succeeded, printing login/setup instructions.
  - `autoActivate: false`: runs the native Git unguarded until
    `/usr/local/share/commitguard/setup` is run manually.
  - After activation: execs the installed guarded Git.
- If there is no repository, an unsafe layout or no verified human login, setup fails and Git stays blocked. After successful activation, missing or changed credentials/cache and incorrect main identities cannot commit; pushes always revalidate the account online. Authenticate using `gh auth login -h github.com`, then retry failed setup with `commitguard-devcontainer-setup`. Refresh a missing or changed identity cache with `commitguard --strict account`.

## Limitations

Normal identity checks use a private cache bound to the local gh credentials,
selected account, configuration directory and gh executable; they make no API
request. `commitguard --strict account` initializes or refreshes the cache online.
Push/pre-push and `fix --apply` always verify online without cache fallback.
Cached checks cannot detect server-side revocation until the next strict check.

The standalone linter supports all 38 Commitlint 21.2.3 message rules through
JSON/YAML configuration. It searches from the current directory to the root for
`.commitlintrc.json`, `.commitlintrc.yaml` or `.commitlintrc.yml`; the nearest
directory wins, with JSON, YAML, then YML priority. `--config FILE` selects an
explicit file. Unsupported JavaScript configuration, plugins and custom parser
regular expressions fail explicitly. Without configuration it uses the fixed
ASCII/LF Conventional Commit policy and whole-message 128-character limit.

CommitGuard keeps its independent identity and message policy. Validated final
`Co-Authored-By` trailer lines and their otherwise empty separator are excluded
from its 128-character count; the complete message still undergoes ASCII and
attribution validation. Other trailers remain counted. Repository lint
configuration does not alter CommitGuard policy.

The shim only covers shells whose `PATH` includes the feature directory; absolute
`/usr/bin/git` bypasses it. Activation is per start and per workspace repository
found from the start directory.

Activation preserves existing hooks and adds the managed policy block to workspace `AGENTS.md` and `CLAUDE.md`. It creates native skills in workspace `.codex/skills/gh-commit-identity` and `.claude/skills/gh-commit-identity`; exclude these generated binary directories from source control. Shell configuration and global Git hooks are installed in the container user's private HOME. Native v0.3.0 also sets workspace-local devflow policy keys; those paths refer to this container.

The workspace must already be a Git repository. Initialize it before opening the container. A ready marker records the last successful installation; Git remains guarded or blocked if a later startup fails; the ready marker is only replaced after successful verification.
