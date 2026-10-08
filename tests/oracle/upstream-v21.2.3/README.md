These nine test files are verbatim from conventional-changelog/commitlint
v21.2.3, downloaded at GitHub-resolved source commit
95d40569d2592bf9719bc27da2d51fc6b801e4ef. The annotated tag object is
384960a8f54c2f0524c8ea5e89d5a919274239bd; GitHub git/tags resolves it to
the source commit. This is a source archive, not a Git
checkout. The adjacent LICENSE retains the upstream MIT notice.

assertions.json contains 86 reviewed argument bindings in source test order.
It is an explicit allowlist, not a generic JavaScript parser. Each record ties
an input, when/value arguments and expected boolean to an original title,
call, assertion line, and file SHA-256. The generation script freezes the
reviewed binding manifest SHA-256 and checks all original source bytes and
assertion locations before executing the pinned upstream rule functions.
Diagnostic text is independently captured from those functions; the original
selected tests assert booleans, so this is oracle output, not a claim that the
original tests asserted every diagnostic string.

The final two footer-max-line-length titles mention multiple lines, but their
actual calls repeat parsed.short and parsed.long. The fixtures intentionally
repeat those actual inputs rather than substituting unused declarations.

Regenerate from the repository root after installing the development-only
pinned oracle dependencies with `bun install --cwd tests/oracle --frozen-lockfile`:

    bun tests/oracle/generate.mjs
    bun tests/oracle/generate-upstream.mjs
    node --test tests/oracle/generate.test.mjs

The first script preserves the existing 221-case fixed-policy corpus. Schema
2 separates upstream lint output and independently failed parsing; an explicit
empty-message-policy deviation keeps our policy stricter than upstream.
The second script writes upstream-rules.json (actual rule assertion reuse)
and upstream-input-policy.json (those raw inputs evaluated under the complete
fixed policy). These are distinct expectations. Rule fixtures use Angular
parser defaults from the original tests. Policy fixtures use the configured
conventionalcommits preset. Their resolved package versions and published
package gitHead metadata are recorded separately from the source archive SHA.

Bun/Node are development-only fixture-generation tools; Rust runtime use and
Cargo fixture tests need neither JavaScript runtime.
