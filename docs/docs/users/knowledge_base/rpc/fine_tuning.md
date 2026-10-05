# Fine-tuning an RPC node

Forest's cache sizes are chosen for a node serving general RPC traffic. If your traffic concentrates on one of the areas below, raising the matching limit trades memory for CPU.

| Variable                               | Default | Suggested | Extra resident memory |
| -------------------------------------- | ------- | --------- | --------------------- |
| `FOREST_TIPSET_TRACE_CACHE_SIZE`       | 16      | 64        | 70-450 MiB            |
| `FOREST_ETH_TRACE_BLOCK_CACHE_SIZE`    | 64      | 2048      | 60-240 MiB            |
| `FOREST_ETH_BLOCK_CACHE_SIZE`          | 500     | 2000      | 35-95 MiB             |
| `FOREST_MESSAGES_IN_TIPSET_CACHE_SIZE` | 8192    | 16384     | 40-80 MiB             |

Entry sizes vary by several times with how busy the tipsets are, hence the wide memory ranges. Raise only what matches your traffic, and confirm against the cache metrics rather than this table.

## Trace methods

`Filecoin.StateReplay`, `trace_block`, `trace_transaction`, `trace_filter`, `trace_replayBlockTransactions` and `debug_traceTransaction` share one cache of traced tipset executions. A miss re-executes the whole tipset with tracing enabled, typically a few hundred milliseconds of CPU. `trace_call` does not use the cache, since it applies a single synthetic message instead. Nor does `debug_traceTransaction` with `prestateTracer`, which replays the tipset up to the target message and is never cached.

`FOREST_ETH_TRACE_BLOCK_CACHE_SIZE` sits in front of it for `trace_block`, `trace_transaction`, `trace_filter` and `trace_replayBlockTransactions`. Its entries are far smaller, so raise it first for those four; raise `FOREST_TIPSET_TRACE_CACHE_SIZE` for `Filecoin.StateReplay` and `debug_traceTransaction`.

Check `cache_eth_trace_block_hits_total` and `cache_tipset_trace_hits_total` against their `_misses_total` afterwards. Concurrent requests for the same tipset are coalesced into one execution but each counts as a miss, so the real hit rate is better than the counters suggest. The `_size_bytes` gauges show what it is costing you; they refresh at most every five minutes.

## Ethereum block methods

`eth_getBlockByNumber` and `eth_getBlockByHash` build their response from a whole tipset and cache it, one entry per tipset. `FOREST_ETH_BLOCK_CACHE_SIZE` defaults to 500, about four hours of chain, so a client walking a day's range evicts everything it needs before coming back to it. Raising it to 2000 covers roughly seventeen hours.

Check `cache_eth_block_full_tx_hits_total` against `cache_eth_block_full_tx_misses_total` afterwards. The setting covers the hashes-only form as well, and the memory figure above accounts for both. Capacity is rounded up internally, so `cache_eth_block_full_tx_cap` reads 512 for a setting of 500.

## Message lookups

`FOREST_MESSAGES_IN_TIPSET_CACHE_SIZE` holds a tipset's decoded messages. The default of 8192 tipsets is about 2.8 days of chain.

This is the most traffic-dependent of the four, so measure before and after with `cache_msg_in_tipset_hits_total` and `cache_msg_in_tipset_misses_total`. Note the cache is not RPC-only: tipset execution, the message pool and gas estimation all read through it, so the hit rate moves with more than your clients.
