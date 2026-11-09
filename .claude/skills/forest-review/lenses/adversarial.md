# Adversarial lens

Done when every input-derived size, depth, count, and range in reachable changed code has a cited bound, an enforcement site, and the mitigation considered, and every changed function has a reachability verdict.

Input surfaces and their roots: `references/entry-points.md`. Roots the diff touches directly: `roots.txt`. Existing limits: `bounds.txt`, a starting inventory to check, not proof of enforcement. Severity, the mitigation rule, and evidence: `references/report-format.md`.

## Reachability

Walk callers of each changed function (`LSP` incoming calls outside PR mode, else grep for the name, trait method, and macro registration) toward a root in `entry-points.md`, stopping at the first root reached or after 6 hops. List one verdict per function under `## Reachability` in `adversarial.md`:

- `reachable from <root> via <caller chain>`
- `not reachable within N hops` (name the hops walked)
- `unknown` (name where the walk stopped)

Unreachable is a verdict only with the walk behind it. Functions marked `unknown` stay in the list and get no rows in the input-surface table.

## Bounds

For every input-derived size, depth, count, and range in reachable code, find three things:

- The bound: a constant, config value, type limit, or upstream check.
- The enforcement site: the line that rejects or truncates. It runs before the allocation, loop, decode, or expensive check it protects. A bound declared but never enforced, or enforced after the work, is a finding.
- The scope: per caller, per peer, or per request. One bound shared by all callers lets one caller exhaust it for everyone (`shared-bound`).

Whether a bound's value matches Lotus belongs to the parity lens; this lens checks that it is enforced.

## Defect classes

Release builds abort on panic and wrap on integer overflow; the `quick` and `quick-test` profiles trap overflow instead (`Cargo.toml` profiles). Both outcomes on input-derived values are defects.

- `panic`: `unwrap`, `expect`, indexing, or slicing on input-derived values; division or remainder by an input-derived divisor; `as` that truncates or flips sign on an input-derived value (`hint:as-cast` in `precheck.txt` lists those in decode paths); recursion with input-derived depth (nested IPLD or CBOR decode, nested JSON, VM call depth reachable from RPC).
- `overflow`: unchecked arithmetic on input integers (epochs, lengths, counts, offsets, gas); a length that wrapped below zero feeding an allocation or a range.
- `unbounded-alloc`: `with_capacity` or `vec![_; n]` sized from the wire; whole responses or streams collected into memory; decompression without an output cap; clone amplification; maps, caches, channels, or subscriptions keyed by input-derived values (peer IDs cost nothing to mint); metric labels from input; response amplification (small request, large response).
- `unbounded-cpu`: parsing worse than linear (re-parse in a loop, long decimal big-integer strings); caller-chosen epoch, block, or message ranges; per-request VM execution or state loads in a loop; an expensive check (signature, proof, state load) running before a cheap check that would reject.
- `trusted-peer-value`: peer-claimed weight, height, timestamps past the allowed drift, or lengths and counts used before verification.
- `liveness`: an await on a peer or remote service without a timeout (outbound HTTP included: check the client the request uses for a timeout, shared clients included); a lock held across an await; spawned tasks with no owner or shutdown path; per-request connections or file descriptors without a cap; a blocking send on the event loop.
- `poisoning`: caching data before validating it; a transient or local failure (missing state, timeout, our own bug) recorded as a permanent rejection in `src/chain_sync/bad_block_cache.rs`; relaying gossip before validation; `MessageAcceptance::Reject` where the fault may be ours or transient, which penalizes honest peers in gossipsub scoring (use `Ignore`).
- `permission`: a method exposing state, side effects, or expensive work at a lower `PERMISSION` than its siblings with the same reach (`hint:permission` in `precheck.txt` lists each change).
- `secret-leak`: JWTs (including the token the F3 sidecar holds), keystore material, or local paths in logs, RPC errors, or metrics.

## F3 sidecar

`f3-sidecar/` is Go running in-process: a Go panic or OOM stops the node. Check `go` statements that start work with no exit path, unbounded buffers and channels, panics on peer-supplied data (index out of range, nil map write, failed type assertion), and message size limits on its gossipsub topics.

## Mitigation

Each finding's `mitigation` names the trust boundary or mitigation ruled out: data from a verified source (signature checked first, hardcoded actor CIDs, bundled assets checked by hash), consensus rejecting the result before use, a bound earlier on the path, an admin-only `PERMISSION`, Forest not producing blocks.

## Disclosure hygiene

Write `claim` and `fix` as a defect class and the change that closes it, never as a recipe: no "attacker", "malicious", or "exploit", and no triggering input. Changelog wording for such fixes: `references/hygiene.md`.

## Input-surface table

In `adversarial.md`, the table defined in report-format.md "Lens report", one row per input-derived size, depth, count, or range in reachable changed code. A row with `none` under "Enforced at" has a matching finding.
