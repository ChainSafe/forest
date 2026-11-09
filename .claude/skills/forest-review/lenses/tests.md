# Tests lens

Done when every changed behaviour has a row under `## Test verdicts` in `tests.md`: behaviour, test (nextest filter), verdict (`covered`, `missing`, or `not worth a test` with the reason), and for a covered fix the plausible bug that turns the test red.

Severity and evidence: `references/report-format.md`. The skeptic picks mutation candidates from the `covered` rows of `## Test verdicts`; a test this lens marks as covering a fix is one it expects to go red with the fix reverted.

## Proportion

Test effort scales with blast radius times logic complexity.

- `missing-test`: a fix without a case the old code fails; parsing, decoding, and consensus code without thorough cases plus a `quickcheck` no-panic property over arbitrary bytes.
- `disproportionate-test`: tests for logging lines, trivial accessor methods, or plumbing whose failure would show elsewhere; test code far larger than the risk it covers.

## Strength

- `passes-without-fix`: the assertion holds with or without the change (it checks setup, a mock, or a value the fix does not affect). Name the line that would need to change for it to go red.
- `weak-assert`: `is_ok()`, `is_some()`, or non-empty checks where the exact value is known; asserting on a debug string instead of the value.
- `string-assert`: matching error message text. Downcast the typed error (anyhow searches the whole chain) or assert the behaviour; the message is fair game only when its text is the feature (an RPC error message Lotus callers depend on). `precheck.txt` flags the plain `.contains` form; this lens catches the rest.
- `duplicate-test`: tests that differ only in inputs become `rstest` cases; repeated setup becomes a fixture helper; a new test that repeats an existing one is removed.

## Speed and reliability

- `slow-test`: sleeps, timing against wall clock on shared CPUs, or network access; a suite that takes seconds where milliseconds would do. Long suites are gated behind a label or the nightly run.
- `self-skipping-test`: a test that returns early or passes when a file, env var, or service is missing, so CI reports it green without running it.

## Snapshot tests

`snapshot-test`: RPC snapshot tests (`src/tool/subcommands/api_cmd/test_snapshots.txt`) are self-contained and meaningfully named, and their total size and overlap with existing snapshots are justified. Use `insta` where the output shape is the assertion.

## Edge cases

`missing-edge-case`: null round, upgrade activation epoch, `-1` and other sentinel ranges, empty input, peer input that must be rejected, and "nothing written on failure" for code that writes to the database or disk.
