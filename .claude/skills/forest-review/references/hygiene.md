# Hygiene

Main-thread checks, run every round and at every effort level. Done when each section below has a verdict, every finding is written to `<round_dir>/hygiene.jsonl` (lens `hygiene`, severity `nit`, schema in `report-format.md`), and the changelog verdict is in `readiness.md`. Comment style belongs to the design lens and env-var docs to the ops lens; in `quick` mode their mechanical parts come from `precheck.txt`.

## Git hygiene

- Index vs working tree: a file with hunks both staged and not staged (`git diff --cached --name-only` and `git diff --name-only` both list it) means the next commit misses part of the change.
- Stray files: design specs and plans (docs/superpowers) stay out of git; throwaway benches, scratch scripts, logs, and `.forest-review/` stay out of the diff. Check tracked additions in `files.txt` and the `untracked` list in `meta.json`.
- Generated files: `docs/openrpc-specs/` and `docs/docs/users/reference/cli.md` are regenerated daily by `.github/workflows/docs-auto-update.yml` in its own PR, so a diff that touches them is a finding.

## Changelog

Rules for entries live in the guide at the top of `CHANGELOG.md`; check against them plus:

- Needed only for user-facing changes (RPC, CLI, config, env vars, behaviour operators see). Internal restructuring, tests, and CI need none.
- Section under `## Forest unreleased`: `### Breaking`, `### Added`, `### Changed`, `### Removed`, or `### Fixed` (a fix restores intended behaviour; a change alters intended behaviour).
- Every affected RPC method, flag, or variable is named, and an entry for a changed default or new limit names the env var or flag that restores the old behaviour when one exists.
- Breaking changes are in `### Breaking`.
- No `#XXXX` placeholders; the link points at the issue or PR.
- Concise; for a fix to input handling, worded as a robustness fix without the input that triggers it.

## Scope

Hunks orthogonal to the change's purpose (drive-by cleanups, unrelated renames, formatting churn in untouched code) move to a separate branch. Name the hunks and the split.

## Claims and provenance

- Every number or factual claim in docs and comments is verified (command, measurement, permalink) or removed. Claims in PR text and commit messages belong to the PR and commit checker.
- Code ported or derived from an external source (Lotus, an EIP, a Protocol Buffers definition, a crypto reference implementation) carries a pinned link: a tag or commit SHA, never a branch.

## Prose

No em-dashes in added comments, docs, changelog entries, or PR text.
