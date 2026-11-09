# Max checks

Contents:

- Prerequisites and consent
- Dispatch and teardown
- Differential check
- Devnet repro
- Fuzz runner

Read by the main thread in `max` only. Angle-bracket placeholders are the SKILL.md ones; fill each template as prompts.md "Filling a template" says.

## Prerequisites and consent

SKILL.md step 1 item 6. Run `${CLAUDE_SKILL_DIR}/scripts/max-prereqs.sh > <round_dir>/max-prereqs.txt`, adding `--snapshot-dir <dir>` when the user named a snapshot directory.

In PR mode, when `git diff --name-only <base_sha> | grep -E '^scripts/(tests/api_compare|devnet)/|(^|/)([^/]*compose[^/]*\.ya?ml|\.env)$'` prints a path, the PR changes the live-check setup: do not offer the live checks, and list them under Gaps with those paths. Otherwise ask the user once whether the live checks may run, naming those whose prerequisites are `ok`; in PR mode say plainly that the PR's code will be built and run in Docker on this machine. The `api_compare` Forest-Lotus comparison and the devnet each build a Docker image of `<wt>`, may read the snapshot named in `max-prereqs.txt` or download one plus proof parameters (the devnet also pulls the Lotus `-2k` image named in `scripts/devnet/.env`), take up to about 2 hours each, and run in their own compose project, refused when the compose file reaches outside it and removed afterwards. Only a yes allows them; on a no or no answer, step 5 runs the other parts and the live ones go under Gaps.

The live checks are experimental (not yet run end to end); say so when asking.

## Dispatch and teardown

SKILL.md step 4 item 4, after the mutation proof, which edits source files. In PR mode with builds requested, append the line `Build env: CARGO_TARGET_DIR=<dir> CARGO_BUILD_BUILD_DIR=<dir>` to each template below; when the user allowed the live checks, append `Live: allowed` to the differential and devnet prompts.

Dispatch in a single message one `forest-review-lens` agent per check, with its template below: "Differential check" when `routing.txt` lists `parity`; "Devnet repro" with live checks allowed, `docker: ok` in `max-prereqs.txt`, and a blocker in `merged.jsonl` that is `confirmed` or `plausible` in `skeptic.jsonl`; "Fuzz runner" when `grep -rl quickcheck` over the directories of the `.rs` files in `files.txt` finds a file. Each returns pass, fail, error, or skipped with a log under `<round_dir>/max/`. Once the live ones have returned, or when one never returns its line, run `${CLAUDE_SKILL_DIR}/scripts/live-project.sh down <round_dir> <check>...` for the live checks dispatched; show the user any `leftover` line and keep it for the readiness block.

## Differential check

```text
Gather differential evidence against Lotus for the change in {round_dir}/diff.patch, applied in the tree at {worktree}. Check name: differential. Scope: the consensus and RPC-output rows of the deviation table in {round_dir}/parity.md; with no such file or rows, nothing is in scope. Done when every row in scope has a pass, fail, or not-covered row.

Inputs: {round_dir}/parity.md, {round_dir}/files.txt, {round_dir}/max-prereqs.txt. Lotus reference: {lotus_rev}.

A row passes only when a test or comparison exercises its behaviour: a test that merely names an RPC method does not cover a consensus row, which needs a compared method such as `Filecoin.StateCompute` over the changed epochs; otherwise the row is not-covered.

1. RPC snapshot tests: for each RPC method in scope, the tests in {worktree}/src/tool/subcommands/api_cmd/test_snapshots.txt that name it, run as `cargo nextest run --cargo-profile quick-test --lib <test name>` from {worktree}, prefixed by the "Build env:" line when this prompt has one. They fill the Forest proof-parameter and RPC test-snapshot caches; name both directories in the .md.
2. Live comparison, only when this prompt has the line "Live: allowed" and max-prereqs.txt shows `docker`, `api_compare stack`, and `lotus image` ok; otherwise write "live comparison not run" with the reason. All Docker state belongs to a review-owned compose project. Never touch another project's containers, volumes, or images, and do not run `setup.sh`. Shell variables do not survive between Bash calls, so start every Bash call that uses them with `P=$({skill_dir}/scripts/live-project.sh name {round_dir} differential); C="docker compose -p $P -f {worktree}/scripts/tests/api_compare/docker-compose.yml --profile include-offline-rpc"`.
   a. Scope gate, before anything is created: `$C config --format json | python3 {skill_dir}/scripts/compose-scope.py --project "$P" --worktree {worktree}`. On a non-zero exit, create nothing, write "live comparison skipped: compose file escapes the review project" with its output lines, and go to the report.
   b. When the `snapshot: ok` line names a `.car.zst` file <dir>/<file>, seed it (read only): `docker volume create --label com.docker.compose.project=$P --label com.docker.compose.volume=node-data ${P}_node-data`, then `id=$(docker run -d --label com.docker.compose.project=$P -v ${P}_node-data:/data -v <dir>:/src:ro --entrypoint cp <image from the lotus image line> /src/<file> /data/)`, repeat `timeout 540 docker wait $id` until it prints 0, then `docker rm $id`.
   c. Start `$(command -v setsid) nohup $C up --build --detach >>{round_dir}/max/differential.log 2>&1 &` (setsid, where it exists, keeps it alive past the Bash call), then repeat `timeout 540 $C wait api-compare api-compare-offline` until both services have exited, `$C ps -a` lists no container, or 2 hours have passed.
   d. Append `$C logs api-compare api-compare-offline` to the log and read the rows for the methods in scope.
   e. On every exit path, including error and timeout: `{skill_dir}/scripts/live-project.sh down {round_dir} differential`. A `leftover` line makes the check error, naming what is left.

Write {round_dir}/max/differential.log (full command output) and {round_dir}/max/differential.md (one row per item: method or path, test or comparison, pass, fail, or not-covered, log excerpt). These are the only files you write; no source edits, commits, or pushes.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "differential: pass|fail|error|skipped, log {round_dir}/max/differential.log" (fail when any row fails, skipped when nothing is in scope).
```

## Devnet repro

```text
Try to reproduce on a local devnet each blocker in {round_dir}/merged.jsonl whose verdict in {round_dir}/skeptic.jsonl is confirmed or plausible, against the tree at {worktree}. Check name: devnet. Done when each such blocker is reproduced, not reproduced, or not reproducible on a devnet (with why).

Inputs: {round_dir}/max-prereqs.txt, the blocker's section in {round_dir}/<lens>.md, {worktree}/scripts/devnet/README.md.

Run only when this prompt has the line "Live: allowed", max-prereqs.txt shows `docker: ok`, and `docker compose version` succeeds; otherwise return error naming the reason. All Docker state belongs to a review-owned compose project. Never touch another project's containers, volumes, or images, and do not run `setup.sh` (it removes volumes by fixed name). Shell variables do not survive between Bash calls, so start every Bash call that uses them with `P=$({skill_dir}/scripts/live-project.sh name {round_dir} devnet); C="docker compose -p $P -f {worktree}/scripts/devnet/docker-compose.yml"`. Before anything is created, return error naming the cause when:
- `docker ps -a --format '{{.Names}}' | grep -xE 'forest|forest-offline|lotus|lotus-miner'` prints a name (the compose file pins these);
- a `*_PORT` value in {worktree}/scripts/devnet/.env is already listening (`ss -ltn`, or `lsof -iTCP -sTCP:LISTEN` without `ss`).
Scope gate: `$C --profile '*' config --format json | python3 {skill_dir}/scripts/compose-scope.py --project "$P" --worktree {worktree} --container-name forest --container-name forest-offline --container-name lotus --container-name lotus-miner`. On a non-zero exit, create nothing and return "devnet: skipped: compose file escapes the review project, log {round_dir}/max/devnet.log" with its output lines in the .md.
1. Start `$(command -v setsid) nohup $C up --build --detach >>{round_dir}/max/devnet.log 2>&1 &` (setsid, where it exists, keeps it alive past the Bash call), then run {worktree}/scripts/devnet/check.sh (read-only, bounded by its own timeouts) until it passes, at most 2 hours in total; stop early when `$C ps -a` lists no container or the log shows `port is already allocated`.
2. Drive each blocker's input through the exposed Forest and Lotus RPC ports or `docker exec forest forest-cli`. Reproduced means the claimed effect is observed: a panic or error in `$C logs forest`, a head or state root that differs from Lotus, or the stated resource growth.
3. On every exit path, including error and timeout: `{skill_dir}/scripts/live-project.sh down {round_dir} devnet`. A `leftover` line makes the check error, naming what is left.

Write {round_dir}/max/devnet.log (full command output) and {round_dir}/max/devnet.md (one row per blocker ID: steps, observed result, log excerpt). These are the only files you write; no source edits, commits, or pushes.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "devnet: pass|fail|error|skipped, log {round_dir}/max/devnet.log" (fail when any blocker reproduces, skipped when there is none; the scope gate has its own line above).
```

## Fuzz runner

```text
Run the property tests that cover the change in {round_dir}/diff.patch, applied in the tree at {worktree}, with far more cases than CI. Check name: fuzz. Done when every changed module with a quickcheck property has been run, and every changed function that parses bytes from peers, RPC callers, or files without one is listed.

Inputs: {round_dir}/files.txt, {round_dir}/roots.txt, {round_dir}/bounds.txt.

Find properties with `grep -rln quickcheck` over the changed files and the test modules next to them. Run each module as `QUICKCHECK_TESTS=10000 QUICKCHECK_MAX_TESTS=100000 QUICKCHECK_GENERATOR_SIZE=100 cargo nextest run --cargo-profile quick-test <module filter>` from {worktree} (CI uses 20 tests at size 20; `quick-test` keeps overflow checks on), prefixed by the "Build env:" line when this prompt has one. A failure records the shrunk counterexample from the output. Do not add tests to the tree; name each missing property and the input it should generate in the .md instead.

Write {round_dir}/max/fuzz.log (full command output) and {round_dir}/max/fuzz.md (one row per property: module, test, cases, pass or fail, counterexample; then the missing properties). These are the only files you write; no source edits, commits, or pushes.

Do the work in this agent; do not dispatch subagents. Return exactly one line: "fuzz: pass|fail|error|skipped, log {round_dir}/max/fuzz.log" (fail on any counterexample, skipped when no property covers the change).
```
