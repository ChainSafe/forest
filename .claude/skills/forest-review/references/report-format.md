# Report format

Contents:

- Round directory
- Admission bar
- Finding schema (`<lens>.jsonl`)
- Stable ID and defect classes
- Severity
- Evidence and Lotus reads
- Lens report (`<lens>.md`) and required tables
- Skeptic output (`skeptic.jsonl`)
- Mutation output (`mutation.jsonl`)
- Return line

## Round directory

`<round_dir>` is `.forest-review/<key>/rounds/<n>/` at the main worktree root; the ledger is `<round_dir>/../../ledger.jsonl` (append-only, the last line per `id` wins) and mutation backups go in `<round_dir>/../../backups/`. `meta.json` holds `base_sha`, `head_sha`, `tree_sha` (before `/simplify`), `reviewed_sha` (the tree reviewed, after `/simplify`), `full_pass`, `mode`, `effort`, and `untracked`.

Lens names: `adversarial`, `parity`, `correctness`, `perf`, `design`, `tests`, `ops`, plus `no-checklist` (no-checklist reviewer), `pr-commit` (PR and commit checker), and `hygiene` (main thread). `merged`, `skeptic`, `mutation`, and `report` are reserved.

## Admission bar

A finding enters `<lens>.jsonl` when all hold: it sits on lines the diff adds or changes, or on code the diff made stale; its evidence is a pointer someone can re-check; its claim is specific enough to refute. Nits meet the same bar. Skip IDs whose latest ledger status is not `open`, and behaviour listed in `known-deviations.md`. Mechanical hits already in `precheck.txt` stay there.

## Finding schema (`<lens>.jsonl`)

One JSON object per line:

```text
{"id": "55f4e5da6b", "lens": "design", "severity": "should-fix", "file": "src/example/mod.rs", "line": 42, "symbol": "Example::load", "class": "duplication", "claim": "...", "evidence": "...", "fix": "...", "verdict": null, "mitigation": null}
```

| Field        | Type        | Rule                                                                                                             |
| ------------ | ----------- | ---------------------------------------------------------------------------------------------------------------- |
| `id`         | str         | stable ID, below                                                                                                 |
| `lens`       | str         | lens name from the list above                                                                                    |
| `severity`   | str         | `blocker`, `must-fix`, `should-fix`, or `nit`                                                                    |
| `file`       | str         | repo-relative path                                                                                               |
| `line`       | int         | line in the reviewed tree                                                                                        |
| `symbol`     | str         | enclosing item path, `Type::method` or `function`; `""` at file level                                            |
| `class`      | str         | defect class slug, below                                                                                         |
| `claim`      | str         | at most 40 words: what is wrong and its consequence                                                              |
| `evidence`   | str         | pointer: `file:line`, pinned permalink, or a command and its decisive output line; never a dump                  |
| `fix`        | str         | concrete change; for a simplicity finding, the smaller alternative itself                                        |
| `verdict`    | null or str | `null` from lens agents; the skeptic writes it to `skeptic.jsonl` and `report.py` copies it to `report.jsonl`    |
| `mitigation` | null or str | the trust boundary or mitigation considered and why it does or does not apply; required for adversarial findings |

## Stable ID and defect classes

`id` = first 10 hex of sha1 over `file|symbol|class`, exactly as `stable_id` in `scripts/state.py`:

```bash
printf '%s|%s|%s' "$file" "$symbol" "$class" | sha1sum | cut -c1-10
```

Line numbers stay out of the ID, so it survives edits above the finding. Pick `class` from this list (first slug that fits; `other` when none does), so the same defect gets the same ID in every round:

- `adversarial`: `panic`, `overflow`, `unbounded-alloc`, `unbounded-cpu`, `shared-bound`, `trusted-peer-value`, `liveness`, `poisoning`, `permission`, `secret-leak`
- `parity`: `consensus-deviation`, `protocol-deviation`, `rpc-deviation`, `bound-deviation`, `unverified-lotus-claim`, `differential-evidence-missing`
- `correctness`: `wrong-result`, `root-cause`, `blast-radius`, `sibling-pattern`, `stale-leftover`, `concurrency`, `boundary`, `upgrade-time-bomb`, `nondeterminism`
- `perf`: `asymptotic`, `redundant-work`, `unbounded-cache`, `large-clone`, `hot-path-alloc`, `blocking-in-async`, `unmeasured-claim`
- `design`: `overengineering`, `duplication`, `reinvented-helper`, `misplaced-logic`, `visibility`, `weak-typing`, `idiom`, `naming`, `narrative-comment`, `missing-why`
- `tests`: `missing-test`, `passes-without-fix`, `weak-assert`, `duplicate-test`, `string-assert`, `slow-test`, `self-skipping-test`, `snapshot-test`, `missing-edge-case`, `disproportionate-test`
- `ops`: `breaking-change`, `data-compat`, `mode-network`, `observability`, `docs-missing`, `ci-house-rule`
- `hygiene`: `changelog`, `scope`, `unverified-claim`, `missing-provenance`, `em-dash`, `git-hygiene`
- `pr-commit`: `claim-mismatch`, `commit-mismatch`, `unexplained-hunk`

## Severity

- `blocker`: consensus or protocol deviation from Lotus; panic, unbounded memory or CPU, or liveness loss reachable from untrusted input with no existing mitigation.
- `must-fix`: an RPC deviation the changelog does not name; asymptotic regression on a hot path; correctness bug; a consensus change without differential evidence (class `differential-evidence-missing`, at any effort level, with how to get the evidence).
- `should-fix`: design, coupling, idiom, test quality.
- `nit`: hygiene and small idiom points.

Reachability alone does not make a blocker: name the mitigation ruled out in `mitigation`, and downgrade when one applies.

## Evidence and Lotus reads

- Forest code: `file:line` in the reviewed tree.
- Lotus: the reference tag `{lotus_rev}`, then master at the commit `{lotus_master}`, both from the Lotus source `{lotus_src}`; your prompt gives all three, resolved once by the main thread, so do not re-derive them or read a local `master` branch. With `{lotus_src}` a path, read `git -C {lotus_src} show <rev>:<path>` for `<rev>` = `{lotus_rev}` or `{lotus_master}`, never checking out, fetching, or changing that clone. With `{lotus_src}` = `gh-api`, read `gh api -H 'Accept: application/vnd.github.raw' 'repos/filecoin-project/lotus/contents/<path>?ref=<rev>'`. Cite `https://github.com/filecoin-project/lotus/blob/<40-hex SHA>/<path>#L<a>-L<b>`: for master the SHA is `{lotus_master}`; for the tag, `git -C {lotus_src} rev-list -n1 {lotus_rev}` (with `gh-api`, `gh api repos/filecoin-project/lotus/commits/{lotus_rev} --jq .sha`). When the tag and master differ, cite both and say which one the Forest change follows. A paraphrase of Lotus without a permalink is not evidence.
- Commands: the command and the one output line that decides the claim.

## Lens report (`<lens>.md`) and required tables

Each finding as a section headed `` `<id>` <severity> <file>:<line> ``, with claim, evidence, fix, and mitigation; then the lens's tables. `report.py` copies these tables into `report.md`:

- `adversarial.md`, input-surface table: `| Entry point | Input-derived parameter | Bound | Enforced at | Mitigation considered |`, one row per size, depth, count, or range in reachable changed code. A bound without an enforcement site reads `none` in "Enforced at" and has a finding.
- `parity.md`, deviation table: `| Behaviour | Forest | Lotus (permalink) | Same? | Changelog |`, one row per changed consensus, protocol, or RPC-output behaviour.

Rows naming an input surface stay in the round directory; draft PR comments and PR text use neutral wording.

## Skeptic output (`skeptic.jsonl`)

One verdict line per finding in `merged.jsonl`:

```text
{"id": "55f4e5da6b", "verdict": "confirmed", "note": "..."}
{"id": "0a1b2c3d4e", "verdict": "confirmed", "note": "...", "duplicate_of": "55f4e5da6b"}
```

`verdict` is `confirmed` (re-derived from the evidence), `plausible` (not refuted, not fully re-derived), or `refuted` (wrong or out of scope; dropped from the report); `note` is at most 30 words naming what was re-checked.

Duplicates: findings that name the same defect (same root cause, whatever their lens or class) form one group. The primary is the one with the highest severity, then the best evidence; the group is judged once, and every other member gets `duplicate_of: <primary id>` and the primary's verdict. A duplicate is never `refuted` for being a duplicate. `report.py` folds each duplicate into its primary, which takes the group's highest severity and names the other lenses (`also: correctness, tests`). Since a duplicate carries its primary's verdict, the duplicates of a refuted primary are dropped with it. A duplicate whose primary is not in `merged.jsonl` is reported on its own. The ledger keeps the folded IDs in the primary's `duplicates` list, and they close with it.

Plus at most 3 mutation candidate lines, where the diff fixes a bug and a test claims to cover the fix:

```text
{"id": "55f4e5da6b", "mutation_candidate": "<nextest filter>", "file": "<path>", "find": "<exact current text>", "replace": "<text that reverts the fix>"}
```

- `mutation_candidate`: the nextest filter selecting the test.
- `file`: repo-relative path of the fixed file; `find` occurs exactly once in it, else the runner records `error`; `replace` reverts the fix.
- `id`: the ID of the finding the candidate tests, or `null` when no finding names it (a regression test the diff adds).
- A `green` result is reported under the candidate's finding ID when that finding is in the report; otherwise `report.py` derives `stable_id(<file>, <mutation_candidate>, "passes-without-fix:" + <find>)`, so two candidates on one function get distinct IDs that stay stable across rounds.

## Mutation output (`mutation.jsonl`)

One line per candidate run, copying `id`, `mutation_candidate`, `file`, and `find` from the candidate line:

```text
{"id": "55f4e5da6b", "mutation_candidate": "<nextest filter>", "file": "<path>", "find": "<text>", "result": "red", "note": "..."}
```

`result` is `red` (the test fails with the fix reverted: it covers the fix), `green` (the test passes without the fix: finding, class `passes-without-fix`), or `error` (build failure, missing test, or a stopped protocol step, named in `note`).

## Return line

Every agent returns exactly one line to the main thread, for example `adversarial: 3 findings (1 blocker, 0 must-fix, 2 should-fix, 0 nit)`; everything else goes to files in `<round_dir>`.
