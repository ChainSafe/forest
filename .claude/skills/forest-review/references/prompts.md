# Prompts

Contents:

- Filling a template
- Lens agent
- No-checklist reviewer
- PR and commit checker
- Skeptic
- Mutation runner

## Filling a template

Each template below is the full prompt for one `forest-review-lens` agent. Replace:

- `{round_dir}`: absolute path of the round directory.
- `{lens_file}`: absolute path of `lenses/<name>.md` (lens agent only).
- `{worktree}`: absolute path of the tree under review (the PR worktree in PR mode).
- `{lotus_rev}`: the Lotus reference tag, for example `v1.38.3`.
- `{lotus_master}`: the 40-hex SHA of Lotus master resolved in SKILL.md step 1.
- `{lotus_src}`: the absolute path of the Lotus clone, or `gh-api` when Lotus is read through `gh api`.
- `{skill_dir}`: absolute path of the skill directory.

Lens agents and the no-checklist reviewer get the filled template and nothing else: no author rationale, plan, or PR description. In PR mode with builds requested, append a line `Build env: CARGO_TARGET_DIR=<dir> CARGO_BUILD_BUILD_DIR=<dir>` to the mutation runner prompt. The `max` check templates are in max.md.

## Lens agent

```text
Review the change in {round_dir}/diff.patch, applied in the tree at {worktree}, through the lens in {lens_file}. The lens name is that file's basename without ".md".

Read first: {lens_file} (its opening completion criterion defines done), then {skill_dir}/references/report-format.md. Inputs: {round_dir}/files.txt, {round_dir}/precheck.txt, {round_dir}/meta.json, the ledger {round_dir}/../../ledger.jsonl, and {skill_dir}/references/known-deviations.md. Read {skill_dir}/references/entry-points.md when the lens file names it.

Source: read and grep under {worktree}; run git as `git -C {worktree}` with read-only subcommands. In PR mode (`mode` is `pr` in {round_dir}/meta.json), do not use the LSP tool: it may start rust-analyzer in {worktree}, which runs the PR's build scripts and proc macros; use Grep and Read. Lotus: {lotus_rev}, then master at {lotus_master}, from {lotus_src}, read as report-format.md "Evidence and Lotus reads" says.

Write {round_dir}/<lens>.jsonl (one finding per line, schema and admission bar in report-format.md; an empty file when nothing meets the bar) and {round_dir}/<lens>.md (finding sections plus the tables the lens file requires). These two files are the only files you write.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "<lens>: N findings (B blocker, M must-fix, S should-fix, K nit)".
```

## No-checklist reviewer

```text
Find what is wrong with the change in {round_dir}/diff.patch, applied in the tree at {worktree}. No checklist: judge correctness, design, tests, and anything else that would make a reviewer push back. Lens name: no-checklist. Done when every changed hunk has been read against its callers and the tests that exercise it.

Read first: {skill_dir}/references/report-format.md. Inputs: {round_dir}/files.txt, {round_dir}/precheck.txt, {round_dir}/meta.json, the ledger {round_dir}/../../ledger.jsonl, and {skill_dir}/references/known-deviations.md.

Source: read and grep under {worktree}; run git as `git -C {worktree}` with read-only subcommands. In PR mode (`mode` is `pr` in {round_dir}/meta.json), do not use the LSP tool: it may start rust-analyzer in {worktree}, which runs the PR's build scripts and proc macros; use Grep and Read. Any claim about Lotus is checked at {lotus_rev}, then master at {lotus_master}, from {lotus_src}, read as report-format.md "Evidence and Lotus reads" says.

Write {round_dir}/no-checklist.jsonl (schema and admission bar in report-format.md; an empty file when nothing meets the bar) and {round_dir}/no-checklist.md. These two files are the only files you write.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "no-checklist: N findings (B blocker, M must-fix, S should-fix, K nit)".
```

## PR and commit checker

```text
Check the PR description and commit messages of the change in {round_dir}/diff.patch against the code in {worktree}. Lens name: pr-commit. Done when every factual claim has a verdict, every commit has been compared with its message, and every hunk is either explained by a stated purpose or listed.

Read first: {skill_dir}/references/report-format.md. Inputs: {round_dir}/meta.json (base_sha, head_sha, mode), {round_dir}/files.txt, the ledger {round_dir}/../../ledger.jsonl. In PR mode, do not use the LSP tool: it may start rust-analyzer in {worktree}, which runs the PR's build scripts and proc macros; use Grep and Read.

Gather:
- Commits: `git -C {worktree} log --format='%H%n%B' <base_sha>..<head_sha>`, and `git -C {worktree} show --stat <sha>` per commit.
- PR text: in PR mode the key directory of {round_dir} is pr-<N>; run `gh pr view <N> --repo ChainSafe/forest --json title,body`. Otherwise run `gh pr view --json title,body` from {worktree}; with no PR, check commit messages only and say so in the .md.

Check:
- Every number and factual claim (measurements, "fixes #N", behaviour, "matches Lotus" checked at {lotus_rev} then {lotus_master} from {lotus_src}) against the diff or re-runnable evidence. Class claim-mismatch.
- Each commit's content against its message, conventional-commit type included. Class commit-mismatch.
- Hunks no stated purpose explains ("why the change?"). Class unexplained-hunk.

Write {round_dir}/pr-commit.jsonl (schema in report-format.md; an empty file when nothing meets the bar) and {round_dir}/pr-commit.md. These two files are the only files you write.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "pr-commit: N findings (B blocker, M must-fix, S should-fix, K nit)".
```

## Skeptic

```text
Try to refute every finding in {round_dir}/merged.jsonl for the change applied in the tree at {worktree}. Done when every finding has exactly one verdict line.

First group the findings that name the same defect: same root cause, whatever their lens or class. Per group pick one primary (highest severity, then best evidence) and judge the group once; write every other member with `duplicate_of` set to the primary's ID and the primary's verdict. `refuted` means wrong or out of scope, never a duplicate (report-format.md "Skeptic output").

Read first: {skill_dir}/references/report-format.md ("Skeptic output" defines the verdicts). Inputs: {round_dir}/merged.jsonl, the finding sections in {round_dir}/<lens>.md, {round_dir}/diff.patch, {round_dir}/bounds.txt, {round_dir}/roots.txt, {skill_dir}/references/entry-points.md, {skill_dir}/references/known-deviations.md.

Source: read and grep under {worktree}. In PR mode (`mode` is `pr` in {round_dir}/meta.json), do not use the LSP tool: it may start rust-analyzer in {worktree}, which runs the PR's build scripts and proc macros; use Grep and Read.

Per primary (a finding with no duplicates is its own primary), re-derive it from source rather than from its text:
- Lotus claims: re-read Lotus at {lotus_rev} and {lotus_master} from {lotus_src} (report-format.md "Evidence and Lotus reads"); a claim the permalinked lines do not support is refuted.
- Reachability: re-trace callers from the changed function to the root in entry-points.md the finding names.
- Bounds: open the cited bound and its enforcement site; check it applies before the allocation, loop, or expensive check.
- Scope: the line is in the diff or in code the diff made stale; the behaviour is not in known-deviations.md.

Nominate at most 3 mutation candidates where the diff fixes a bug and a test claims to cover the fix, starting from the `covered` rows under "## Test verdicts" in {round_dir}/tests.md: the nextest filter, the fixed file, a `find` text that occurs exactly once in it, and the `replace` text that reverts the fix (schema and ID rule in report-format.md "Skeptic output"). Nominate only; leave every source file untouched.

Write {round_dir}/skeptic.jsonl: one verdict line per finding plus the mutation candidate lines, both schemas in report-format.md. This is the only file you write.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "skeptic: C confirmed, P plausible, R refuted, M mutation candidates".
```

## Mutation runner

```text
Prove whether each regression test goes red without its fix. Candidates: the "mutation_candidate" lines in {round_dir}/skeptic.jsonl, at most 3, run serially in the tree at {worktree}. Backups directory: {round_dir}/../../backups/ (empty at start; a backup of <file> is backups/<file>). Hash with `git hash-object <file>`.

Read first: {skill_dir}/references/report-format.md ("Mutation output" defines red, green, error). Each candidate line carries `mutation_candidate` (the nextest filter), `file`, `find`, and `replace`; the mutation replaces the single occurrence of `find` in `file` with `replace`, and each result line copies the candidate's `id`, `mutation_candidate`, `file`, and `find`.

Per candidate:
1. Copy the original file to `backups/` and record its hash.
2. Apply the mutation; record the mutated file's hash.
3. Run the targeted test.
4. Before restoring, confirm the file still has the mutated hash (if not, the user or a tool edited it: stop and report, do not overwrite).
5. Restore from the copy, confirm the original hash, delete the backup, then remove empty directories under `backups/`: `find {round_dir}/../../backups -mindepth 1 -type d -empty -delete`.

No `git checkout`, `git stash push/pop`, or staging.

Step 3 runs `cargo nextest run --cargo-profile quick-test <filter>` from {worktree}, prefixed by the "Build env:" line when this prompt has one. A test that fails on its assertion is red; a test that passes is green; a compile error, a filter matching no test, or `find` occurring other than exactly once in `file` is error (checked before step 1, so nothing is copied or changed). A stop at step 4, or a hash mismatch at step 5, is error with the file named in the note; leave its backup in place and skip the remaining candidates.

Write {round_dir}/mutation.jsonl, one line per candidate (schema in report-format.md). Files you write: that file, the backups, and each candidate's file during its own run.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "mutation: R red, G green, E error".
```
