# Design lens

Done when every added abstraction, knob, helper, constant, and public item has a row under `## Justifications` in `design.md` (item, and its justification or the finding ID with the smaller alternative), and every added function sits in the module that owns its data.

Severity and evidence: `references/report-format.md`. Mechanical rules belong to clippy (`.clippy.toml`, workspace lints in `Cargo.toml`) and `precheck.txt`; this lens covers judgment.

## Simplicity

`overengineering`: propose the smallest diff that solves the problem, concretely, in `fix`. An abstraction or knob is justified by a second real caller or implementation, an operator who will set the knob, or a measured need; flag only what lacks one. Smells when unjustified:

- a trait with one implementation, a generic used with one type, a builder for two fields;
- config knobs nobody sets; `LazyLock` for a value used once; a module-level constant used by one function; a wrapper over a one-liner;
- JSON fixtures where literals do; a diff far larger than the problem;
- leniency every caller pays for: one error case (a 404, an empty body) special-cased when every failure should take the same path; tolerant parsing where strict and fast serves.

## Reuse

- `duplication`: copy-pasted logic a justified abstraction removes; one value (chain constants, URLs, keys) defined in several places instead of once where it belongs (network values under `src/networks/`). The finding sits on the added line and lists the existing sites in `evidence`.
- `reinvented-helper`: before accepting a new helper or constant, grep for the operation and its types (checksum and digest checks, hex, retries, downloads, caches, network constants). Name the existing item in `fix`.

## Coupling and cohesion

- `misplaced-logic`: logic lives in the module that owns its data. A computation that uses no `StateManager` field belongs in `src/shim/` or on the type it computes over, not in `src/state_manager/`; a feature gets its own module, not a utility file (`src/daemon/db_util.rs`) or a growing `src/daemon/mod.rs`; RPC methods call `StateManager` and `ChainStore` APIs rather than copying their logic; large test blocks move to their own file.
- `src/shim/` is the version boundary: its public types and methods are version-free (versioned paths outside it are `versioned-path` in `precheck.txt`).
- `visibility`: the narrowest that works (`pub(crate)`, `pub(super)`, `pub(in path)`); related state grouped in a struct or module without adding layers.
- A function that runs several steps each with its own inputs and result splits at those steps (class `other`); one PR carries one concern (scope splits are hygiene's).

## Idiom

- `weak-typing`: enums over `bool` parameters and stringly-typed values; domain types (`ChainEpoch`, `TokenAmount`) over bare integers; typed errors (`thiserror`) where callers or tests match on them.
- `idiom`: `let-else`; `anyhow::ensure!(cond, ...)` over `if !cond { bail!(...) }` (plain cases are `if-bail` in `precheck.txt`); ownership chosen to avoid clones, `Cow` where it fits; `Arc<[T]>` vs `Vec<Arc<T>>` chosen deliberately; plain bounds over `Borrow` trait gymnastics; iterator chains only where they read better than a loop. `as` on input-derived values belongs to the `adversarial` lens, `format!` cost to the `perf` lens.

## Naming

`naming`:

- An expensive call is not named like a getter (`calc_*`, `compute_*`, `load_*` over a bare noun).
- Names carry no actor or network version (`as_v19`, `get_v16`); the shim hides versions.
- `fetch_*` fetches; `try_*` returns a result; names match what the code does after the change.

## Comments

The house rule: no comment by default; a comment says why in one line, never what or history.

- `narrative-comment`: comments that restate the code, narrate the change or its history, explain Rust or the standard library, or claim parity with Lotus (a pinned permalink replaces the claim when a link is needed at all).
- `missing-why`: a one-line why is missing on a magic value, a crypto constant or test vector (its source: round, chain hash, key origin, or reference link), a fixture, or a convoluted condition. A function callable from async code whose name and signature hide that it blocks (synchronous I/O, waiting on the network or a lock, running the VM) says so in its doc comment.
