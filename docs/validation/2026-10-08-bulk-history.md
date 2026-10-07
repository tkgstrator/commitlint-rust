# Native bulk history validation

Final Mac arm64 executable SHA256:
`de8b6b6092acbaf9affd0330bd3bbb2be98701cc120800b99819ecacfc146d41`.
The separately copied executable is mode 0555; the installed host guard and its
configuration were not replaced. Runtime dependencies remain native Git and gh.

## Verification

- Mac full workspace suite: 191 passed, zero failed, two subprocess fixtures
  ignored; their parent tests exercise them. This preceded the final added human
  certification rejection test.
- Final Mac release bulk/library/tag suite: 71 passed, zero failed, one ignored
  subprocess fixture exercised by its parent. Includes the final rejection test.
- Final Linux bulk/library/tag suite: 70 passed, zero failed, one ignored fixture.
- Final clean Linux full workspace suite: 191 passed, zero failed, two ignored
  subprocess fixtures exercised by their parent tests. The large-hook diagnostic
  deadline test also passed. Both owned disposable test containers were removed.
- Formatting, staged whitespace checks and skill validation passed.
- Independent Claude actual code reviews found receipt-binding, signature/type
  handling and invented human-certification issues; fixes were reviewed again.
  LocalGPT review was unavailable, so it is not claimed here.

Earlier overlapping debug builds correctly invalidated frozen executable hashes.
An older disposable container used a non-reaping PID 1 and accumulated 1,851
zombie processes; its diagnostic deadline test failed. That owned container was
removed and replaced by a fresh container with `--init` for the final run.

## Synthetic benchmark

7,000 linear commits across 80 isolated repositories with empty trees:

| Measurement | Result |
| --- | ---: |
| Fixture preparation | 18.941 s |
| Preview | 213.658 s |
| Write and verify | 84.114 s |
| Bulk total | 300.448 s |
| Git subprocesses | 880 |
| Local gh subprocesses | 1,120 |
| Bulk GitHub API calls | 0 |
| Hook invocations | 0 |

Original refs stayed unchanged. The benchmark used frozen executable SHA256
`cabc6a129841146c7ba93099918a9276739c67e040315c905a8c8a40df20e826`;
the final executable adds the independently tested leftover-human-credit refusal.
These timings exclude LLM wording generation, graph/ref promotion, real trees,
remote transport and push. They do not claim that real repository histories were
published. Fixture directories were removed; the result log was retained.

The Commit Fix task received the final executable path, SHA256, exact manifest
contract and responsibility for backups, expected-old ref transactions, graph
verification and separately authorized publication. Colliding normalized OIDs
remain held; dates, nonces or squashing were not invented to resolve them.
