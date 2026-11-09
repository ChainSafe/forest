# Ops lens

Done when every user-visible change (CLI flag, config key, env var, RPC method, default, limit, metric, log line, on-disk format) has a row under `## Compatibility` in `ops.md` (change, `compatible` or `breaking`, changelog `present`, `needed`, or `n/a`), and, when the diff touches `.github/`, `scripts/`, a Dockerfile, `mise.toml`, or `mise-tasks/`, every changed job, image, and script has been checked against the CI section below.

Severity and evidence: `references/report-format.md`. What a changelog entry contains, breaking ones included: `references/hygiene.md`.

## Compatibility

- `breaking-change`: renamed or removed CLI flags, config keys, env vars, RPC methods, or metrics; changed defaults; new limits that reject what used to succeed.
- `data-compat`: existing data directories keep working; a migration path for any format change; crash consistency (`flush` or `sync_all` after `write_all`, write then rename); behaviour on garbage-collected state; recovery gentler than deleting the database.
- `mode-network`: `--stateless` mode; network-specific config for mainnet, calibnet, devnet, and butterflynet; configuration variants the code handles only partly (chained vs unchained Drand networks, F3 on or off, signature schemes). An unsupported variant fails with an error that names it, and the limit is stated where it is enforced.

## Observability

`observability`:

- Log level matches what an operator can do: `warn` and `error` only when there is an action.
- A log line a peer or RPC caller can trigger at will is rate-limited or at `debug`.
- Metric label sets stay bounded; counters sit at the event they count.
- Errors name what failed (which URL, bundle, file, or peer) and what happens next (retry, fallback, abort).

## Docs

`docs-missing`:

- New and changed `FOREST_*` variables are in `docs/docs/users/reference/env_variables.md` with their default, effect, and when to set them (`precheck.txt` lists names missing from the file; this lens checks the description).
- CLI help text and `value_name` describe the argument.
- Operator docs stay concise: no internals, derivations, or sample output that needs upkeep.

## CI, Docker, scripts

`ci-house-rule`:

- Versions pinned (images, actions, tools); `:latest` is in `precheck.txt`.
- Long flags in scripts (`--silent`, not `-s`).
- apt installs in workflows run under `nick-fields/retry`, as the existing Apt Dependencies steps do.
- Build and test logic runs inside the container, not in the workflow around it.
- No hardcoded or exposed ports.
- Scripts are shellcheck-clean; inline bash in workflows stays short, longer logic moves to `scripts/`.
- Every job has a timeout sized to its normal run, so a stuck job releases the runner.
- CI wall time: a new check joins an existing job where it fits; a long one is gated (label, schedule, or path filter).
- Ordering between workflows holds (an image a dependent job pulls is published before that job runs).
- Output needed to debug a failure is not sent to `/dev/null`.
- Commented-out code is removed.
- Generated files are marked in `.gitattributes`.
