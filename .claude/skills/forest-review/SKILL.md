---
name: forest-review
description: Forest code review with parallel lens agents, a skeptic, and a findings ledger. Use for a self-review of the current branch before a PR, for reviewing a ChainSafe/forest pull request by number, and for a re-review after fixing findings from an earlier round.
argument-hint: "[quick|full|max] [PR#|--delta] [--lens name]"
allowed-tools:
  - Bash(${CLAUDE_SKILL_DIR}/scripts/*)
  - Bash(python3 ${CLAUDE_SKILL_DIR}/scripts/*)
  - Bash(git status:*)
  - Bash(git diff:*)
  - Bash(git log:*)
  - Bash(git show:*)
  - Bash(git rev-parse:*)
  - Bash(git merge-base:*)
  - Bash(git ls-files:*)
  - Bash(git cat-file:*)
  - Bash(git worktree list:*)
  - Bash(gh pr view:*)
  - Bash(gh pr diff:*)
  - Bash(mise lint:*)
  - Bash(mise run lint*)
hooks:
  PreToolUse:
    - matcher: "Bash"
      hooks:
        - type: command
          command: 'for f in "${CLAUDE_PROJECT_DIR}/.claude/skills/forest-review/scripts/guard.py" "$HOME/.claude/skills/forest-review/scripts/guard.py"; do [ -f "$f" ] && exec python3 "$f"; done; exit 0'
---

# Forest review

Arguments: `$ARGUMENTS`

Current branch against `origin/main` (the session checkout; meaningful in self and delta mode only, ignore it in PR mode; empty when there is no change, empty or a git error line when there is no `origin/main`): !`git diff --stat origin/main...HEAD | tail -1 || true`

The main thread prints only the ranked findings and the report path; agent output, tables, and lint logs stay in files under the round directory. Guardrail: the `hooks` block above stops commits, pushes, and GitHub writes for the rest of this session; it is best-effort command matching, not a security boundary, and whether it fires inside dispatched agents is undocumented, so only `forest-review-lens` agents, which carry their own copy, count as guarded.

## Branches

Parse the arguments first; a step heading names the efforts or modes it runs in, and a step item names the modes it applies to.

- Effort: `full` (default) runs steps 1 to 5. `quick` runs steps 1, 2, and 5: no fan-out, skeptic, or mutation proof, and a failing `cargo check` never blocks. `max` runs `full` plus step 1 item 6 and the differential, devnet, and fuzz checks of step 4 item 4, each only when its prerequisites are present and the live ones only after the user's yes; the live checks are experimental (not yet run end to end), so say so when offering them.
- Mode: self (no target) reviews the current branch against its merge-base with `origin/main`. `--delta` is self mode for a re-review after fixes. `PR#` (a number) is PR mode: the PR in a separate worktree, no build, lint, mutation proof, or fixes unless the user asks (builds then use `CARGO_TARGET_DIR=<key_dir>/target CARGO_BUILD_BUILD_DIR=<key_dir>/build`), and findings drafted as review comments that are never posted.
- `--lens a,b`: dispatch only the named lenses (names in `${CLAUDE_SKILL_DIR}/references/report-format.md` "Round directory"), routed or not; the skeptic still runs.

Placeholders: `<wt>` is the tree under review (the top level of this checkout, or the PR worktree), `<main>` is the first `worktree` path of `git worktree list --porcelain`, `<key>`, `<round>`, `<round_dir>`, `<base_sha>`, `<tree_sha>` come from step 1, and `<key_dir>` is `<round_dir>/../..`. Run every command from `<wt>`.

## 1. Scope

1. PR mode only, run from `<main>`: `gh pr view <N> --repo ChainSafe/forest --json headRefOid,baseRefName`, then `git fetch <remote> pull/<N>/head <baseRefName>`, where `<remote>` is the remote whose URL is ChainSafe/forest (`git remote -v`). `<wt>` is `<main>/.forest-review/pr-<N>/worktree`: `git worktree add --detach <wt> <headRefOid>`, or `git -C <wt> checkout --detach <headRefOid>` when it already exists.
2. Start the round: `python3 ${CLAUDE_SKILL_DIR}/scripts/state.py init-round --mode self|delta|pr --effort <effort>`, adding `--pr <N> --base <remote>/<baseRefName>` in PR mode. It creates `<round_dir>` with `meta.json` and prints one JSON object (`key`, `round`, `round_dir`, `base_sha`, `head_sha`, `tree_sha`, `full_pass`). `tree_sha` is the working tree under review, uncommitted edits included.
   - Exit 3: `<key_dir>/backups/` still holds files from an interrupted mutation proof. Show the listed files, ask the user to compare each with its source and restore or delete it, and stop.
   - Exit 4: no merge-base with the base; ask the user for a base ref and stop.
   - Any other non-zero exit: show its stderr and stop.
3. Diff base `<diff_base>`: `<base_sha>`. In delta mode with `full_pass` false, use `tree_sha` from `<key_dir>/rounds/<round - 1>/meta.json` when `git cat-file -e <sha>` succeeds; otherwise use `<base_sha>` and say why in the readiness block.
4. Lotus, resolved once here for every agent. `<lotus_rev>` is the `LOTUS_IMAGE` tag in `scripts/tests/api_compare/.env` without its `-<network>` suffix. With a clone at `${LOTUS_SRC:-<main>/../lotus}` (`git -C <dir> rev-parse --git-dir` succeeds), `<lotus_src>` is that path: run `git -C <lotus_src> fetch --quiet origin master --tags` (fetch only; never check out or reset there), then `<lotus_master>` is `git -C <lotus_src> rev-parse origin/master`; when the fetch fails, take that local ref (`master` when it is missing) and note it under Gaps with its commit date. When there is no clone, or `<lotus_rev>^{commit}` does not resolve in it, `<lotus_src>` is `gh-api` and `<lotus_master>` is `gh api repos/filecoin-project/lotus/commits/master --jq .sha`; tell the user once to point `LOTUS_SRC` at a Lotus clone. Agents read a clone outside this project only when the user allows it (`--add-dir` or a project allow rule).
5. Index overlap (self and delta): note the files listed by both `git diff --cached --name-only` and `git diff --name-only`, for the hygiene check in step 2.
6. `max` only (PR mode: only when the user asked for builds): read `${CLAUDE_SKILL_DIR}/references/max.md` and do "Prerequisites and consent".

Done when `<round_dir>` exists and the diff base, `<lotus_rev>`, `<lotus_src>`, `<lotus_master>`, the index overlap, and in `max` the live-check answer are known.

## 2. Prepare

1. Diff: `git diff --no-color --no-ext-diff <diff_base> > <round_dir>/diff.patch` and `git diff --name-only <diff_base> > <round_dir>/files.txt`. For each new file git does not track yet that belongs to the change (`git ls-files --others --exclude-standard`), append its path to `files.txt` and `git diff --no-color --no-index /dev/null <file>` to `diff.patch` (that command exits 1 whenever it prints a diff).
2. Mechanical checks: `${CLAUDE_SKILL_DIR}/scripts/precheck.sh --base <diff_base> --out <round_dir>`. It writes `precheck.txt`, `routing.txt`, `roots.txt`, and `bounds.txt` and prints one summary line; exit 2 means a bad base, so report it and stop.
3. Build gate (self and delta): `cargo check --all-targets --message-format short 2>&1 | tail -20`. On failure, keep the output for the readiness block and continue as `quick`.
4. Lint (self and delta), started in the background with `run_in_background`: `${CLAUDE_SKILL_DIR}/scripts/lint-summary.sh --out <round_dir>/lint.txt lint:rust-fmt lint:clippy lint:spellcheck lint:rustdoc`, adding `lint:shellcheck` when `files.txt` lists a shell script and `lint:actions` when it lists a file under `.github/workflows/`. Its pass/fail/skip summary is the only lint output that enters this context.
5. Hygiene: read `${CLAUDE_SKILL_DIR}/references/hygiene.md` and run every section on `files.txt`, `meta.json`, the index overlap, and `git diff --stat <diff_base>`, plus targeted reads of the `CHANGELOG.md` and docs hunks and the comment-bearing hunks; findings go to `<round_dir>/hygiene.jsonl`. Read at most about 400 lines of `diff.patch` here; beyond that, work from the stat and, in step 5, from the lens reports.

Done when `diff.patch`, `files.txt`, `precheck.txt`, `routing.txt`, and `hygiene.jsonl` exist, and in self and delta mode the build gate has a result and lint is running.

## 3. Fan out (`full` and `max`)

Read `${CLAUDE_SKILL_DIR}/references/prompts.md` and fill each template with absolute paths: `{round_dir}`, `{worktree}` = `<wt>`, `{lotus_rev}`, `{lotus_src}`, `{lotus_master}`, `{skill_dir}` = `${CLAUDE_SKILL_DIR}`, and per lens `{lens_file}` = `${CLAUDE_SKILL_DIR}/lenses/<lens>.md`. Send each agent its filled template and nothing more (prompts.md "Filling a template").

Dispatch in a single message, one Agent call each with `subagent_type: forest-review-lens`:

- the "Lens agent" template for every lens in `routing.txt`;
- the "No-checklist reviewer" and "PR and commit checker" templates.

With `--lens`, dispatch only the named ones. When the `forest-review-lens` type is unavailable (a session started before the skill was installed), dispatch `general-purpose` agents with the same prompts and tell the user the guard hook does not cover them.

Done when every agent has returned its one-line count and its `<lens>.jsonl` exists. Re-dispatch an agent once when its file is missing; after that, list the lens as not reviewed in the readiness block.

## 4. Verify (`full` and `max`)

1. Merge: `python3 ${CLAUDE_SKILL_DIR}/scripts/report.py merge <round_dir>` writes `merged.jsonl`: duplicates across lenses merged by ID, IDs closed in the ledger dropped.
2. Skeptic: when `merged.jsonl` is non-empty or `<round_dir>/tests.md` has `covered` rows under `## Test verdicts`, dispatch one `forest-review-lens` agent with the "Skeptic" template. It writes `skeptic.jsonl` with a verdict per finding (cross-lens duplicates marked `duplicate_of`) and at most 3 mutation candidates.
3. Mutation proof (self and delta; PR mode only when the user asks, with the `Build env:` line from prompts.md): when `skeptic.jsonl` has `mutation_candidate` lines, wait for lint to finish, then dispatch one `forest-review-lens` agent with the "Mutation runner" template (the step 3 fallback applies to it and the skeptic). Afterwards `<key_dir>/backups/` is empty; when it is not, name its files in the readiness block and tell the user to restore them before the next round.
4. Max checks (`max`; PR mode only when the user asks for builds, with the `Build env:` line): after the mutation proof, which edits source files, do max.md "Dispatch and teardown". A result is evidence for the readiness block, not a finding: a consensus change keeps its `differential-evidence-missing` must-fix unless a `pass` row in `max/differential.md` covers it, and the user then rejects the finding citing that row.

Done when every merged finding has a verdict and every candidate a result in `mutation.jsonl`, or the step is recorded as skipped with its reason; in `max`, every dispatched check has returned its line.

## 5. Report

1. Write `<round_dir>/readiness.md` (in self and delta mode, after the lint summary arrives):
   - ELI5 of the change, three sentences at most, from the step 2 inputs and the lens reports.
   - Changelog verdict from the hygiene check: needed or not, section, draft entry.
   - Mutation proof: red, green, or error per candidate, or "not checked" with the reason (`quick`, PR mode, no regression test, or no findings and no covered tests).
   - Memory and performance numbers where the change claims or risks them, or which numbers are missing.
   - Suggested PR splits.
   - `cargo check` and lint status, pointing at `lint.txt` for failures; a `skip` is not a failure, so list it with its reason (untrusted mise config with the `mise trust` command to run, or a task absent at this revision); "not run (PR mode)" in PR mode.
   - Mechanical checks: the summary line `precheck.sh` printed, then the lines of `precheck.txt` without `hint:`, at most 30, followed by `... <K> more in precheck.txt` when there are more.
   - `max` checks: each check's return line and its `max/<check>.md`, or why it was not dispatched; the cache directories the differential check names, which RPC snapshot tests fill.
   - PR mode cleanup, for when the user is done with the PR: `git worktree remove <wt>`, plus `rm -rf <key_dir>/target <key_dir>/build` when builds ran; the ledger stays for later rounds.
   - Gaps: lenses not reviewed, Lotus read through `gh api` or master from an unfetched local ref, a full diff in delta mode, and in `max` the live checks not allowed, not offered because the PR changes their setup, or skipped because the compose file escapes the review project, any `leftover` line from `live-project.sh`, and every `missing` line of `max-prereqs.txt` with the check or part it held back.
2. Render: `python3 ${CLAUDE_SKILL_DIR}/scripts/report.py render <round_dir>`, adding `--pr` in PR mode, writes `report.md` and `report.jsonl` (the reported findings with their skeptic `verdict`: refuted ones dropped, duplicates folded into their primary, mutation-derived ones added) and prints the ranked one-line findings and the report path; past 15 findings it prints every blocker and must-fix, should-fix up to 15 lines, and a `+N more` line naming `report.md`. With `--pr` it appends `## Draft review comments`, one per finding with its `file:line`, input-handling ones worded neutrally; nothing is posted. Show the user exactly that output, then in `max` each dispatched check's return line.
3. Convergence: a blocker or must-fix in `report.jsonl` (the ranked list) whose `verdict` is `confirmed` or `null` (`quick`, or a finding the skeptic skipped) is new when `grep -c '"<id>"' <key_dir>/ledger.jsonl` finds nothing (that also matches IDs folded as duplicates). Then record the round: `python3 ${CLAUDE_SKILL_DIR}/scripts/state.py ledger-upsert <key> <round_dir>/report.jsonl`.
   - In `max`, a check that returned `fail` counts as a new blocker or must-fix (a fuzz counterexample as a blocker); name its `max/<check>.md`.
   - No new blocker or must-fix: the review has converged; say so.
   - Otherwise, before round 3: say the round has not converged. From round 3 on: say three rounds have not converged and stop suggesting further rounds.
4. Ask the user to mark findings to fix, reject, or defer, with a reason for reject and defer, and record each: `python3 ${CLAUDE_SKILL_DIR}/scripts/state.py ledger-set <key> <id> accepted|rejected|deferred [--reason "<text>"]`. In self and delta mode, end by saying that `fix <ids>` or `fix all` applies fixes. The review itself leaves the tree as it found it.

Done when the ranked findings and the report path are shown and every decision the user gives is in the ledger.

## 6. Fix (self and delta, on request)

Run only when the user names fixes: `fix <ids>`, or `fix all` for every reported finding not rejected or deferred. Agreeing with the report or marking findings `accepted` is not a request to fix.

Mark each named finding `accepted` when it is not already, apply it in the working tree, then mark it: `python3 ${CLAUDE_SKILL_DIR}/scripts/state.py ledger-set <key> <id> fixed`. Rerun `lint-summary.sh` with the step 2 tasks and `--out <round_dir>/lint-fix.txt`. Changes stay uncommitted for the user to review.

Done when every named finding is fixed or reported as not fixable, and the lint summary is shown. End with `/forest-review --delta` as the next round, unless the step 5 stop rule applies.
