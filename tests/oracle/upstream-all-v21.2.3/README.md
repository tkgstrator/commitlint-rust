# Full upstream rule-test capture

The 80 hashed files reproduce the upstream v21.2.3 rule directory, its reference
rule documentation, and license. The source commit is
95d40569d2592bf9719bc27da2d51fc6b801e4ef; annotated tag object
384960a8f54c2f0524c8ea5e89d5a919274239bd resolves to that commit. The manifest
and every source byte are checked before transpilation.

Regenerate from the repository root:

```sh
node --experimental-vm-modules tests/oracle/generate-full-upstream.mjs
node --experimental-vm-modules --test tests/oracle/*.test.mjs
```

The reference requires Node v26.8.2 (Unicode 17.0) and Bun 1.3.6. Bun only
transpiles the original TypeScript to Node-targeted JavaScript and source maps.
All regexes, case transformations, original test callbacks, and published pinned
Commitlint rule functions execute under Node. This avoids Bun's different
Unicode version. These are development dependencies; the Rust runtime does not
invoke this harness.

A narrow fail-closed Vitest compatibility layer registers and expands the
original describe/test/it/each calls. It actually evaluates the selected source
matchers: toEqual, toBe, toBeTruthy, toBeFalsy, toContain and arrayContaining.
Unknown imports/matchers, failed assertions, changed source bytes, missing Git,
or unfinished callbacks refuse generation. All 532 functional callbacks and
three original registry/documentation checks execute serially. VM identifiers
preserve original source URLs; Bun source maps recover original registration,
rule-call and assertion locations after transpilation.

full-upstream-rules.json schema 3 separates actual assertions from observed
rule outcomes. Each flattened rule call links to its original test, expanded
parameters, parser input/options, parsed six fields/raw/references, typed
when/value arguments, and observed result. Undefined and null remain distinct;
missing and empty diagnostic messages remain distinct. RegExp values in parser
metadata retain source/flags. Message-only assertions do not imply that the
original source also asserted the observed Boolean outcome.

The original trailer-exists rule still calls Git. Its actual read-only
interpret-trailers subprocess uses an explicit Git path, a private HOME/XDG/cwd,
no inherited GIT_* configuration, and disabled global/system configuration.
Execution is bounded to ten seconds and one MiB of output; nonzero status,
interruption or spawn failure refuse capture. Exact Git version and successful
operation records are retained. Temporary directories, process environment,
working directory and the subprocess instrumentation are restored on failure
and success. The default reference Git path is /usr/bin/git on this Mac.

532 rule calls and 632 actual expectations are currently captured. The three
metadata tests have their own expectation evidence without pretending to be
Rust rule inputs. The older 221-case fixed-policy corpus and reviewed 86-case
manual assertion corpus remain separate and unchanged in format.
