# Performance lens

Done when every changed function on a hot path has its cost under `## Costs` in `perf.md` (complexity in the input that grows, work repeated per call, blocking calls), every new cache and every cache whose size changes has its maximum size and memory cost there, and every speedup or memory claim has a number or a finding.

Severity and evidence: `references/report-format.md`.

## Hot and cold

- Hot path: RPC handlers (above all `eth_*`, trace and replay, `eth_call` and `eth_estimateGas`, logs, receipts), sync and block validation, mpool, gossip handling, blockstore reads, CBOR encode and decode, state-tree walks.
- Cold: CLI, startup, tests. Migrations run once but stay bounded.

Report asymptotic issues and obvious waste on a hot path, and memory held by a new cache on any path; micro-optimization in cold code is not a finding.

## Checklist

- `asymptotic`: a scan where a direct lookup exists (an AMT or HAMT get instead of a walk over all entries); a state-tree walk per request; nested loops over a tipset's messages or receipts.
- `redundant-work`: VM execution where a direct state read answers; recomputation a memo or an existing cache already holds; verifying again what a cache stores as verified (signatures, validated blocks or tipsets) instead of the constructor that trusts the cache; the same tipset or state loaded repeatedly in a loop.
- `unbounded-cache`: a cache without a stated maximum, or sized beyond what the method's call rate and hit rate justify (a cold method gets a small cache or none). Bounded by entries or bytes (`SizeTrackingCache`, `src/utils/cache/size_tracking.rs`); memory cost computed from entry size or measured; eviction under concurrent churn considered.
- `large-clone`: per-call clones of large structs, vectors, or maps where `Arc` or a borrow works.
- `hot-path-alloc`: `format!` and `to_string` on a hot path; `Vec` without `with_capacity` when the length is known; intermediate collections where an iterator works. `.context(format!` is `precheck.txt`'s `context-format`.
- `blocking-in-async`: VM execution, synchronous I/O, or heavy CPU on an async task without `spawn_blocking`; a `Mutex` where an atomic suffices. A lock held across an await is the adversarial lens's `liveness`.
- `unmeasured-claim`: a speedup or memory claim without numbers; an RPC change that alters cost on a hot path (new state loads, VM calls, caching, loops) without before and after latency for the methods it touches; complexity added for performance without a measurement. The repo has no RPC load benchmark, so the finding names what to measure and how.
