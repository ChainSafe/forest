# Fine-tuning an RPC node

Forest's cache sizes are chosen for a node serving general RPC traffic. If your traffic concentrates on one of the areas below, raising the matching limit trades memory for CPU.

| Variable                               | Default | Suggested | Extra resident memory |
| -------------------------------------- | ------- | --------- | --------------------- |
| `FOREST_TIPSET_TRACE_CACHE_SIZE`       | 16      | 64        | 70-450 MiB            |
| `FOREST_ETH_TRACE_BLOCK_CACHE_SIZE`    | 64      | 512       | 20-215 MiB            |
| `FOREST_ETH_BLOCK_CACHE_SIZE`          | 500     | 2000      | 35-95 MiB             |
| `FOREST_MESSAGES_IN_TIPSET_CACHE_SIZE` | 8192    | 16384     | 40-80 MiB             |

Entry sizes vary by several times with how busy the tipsets are, hence the wide memory ranges. Raise only what matches your traffic, and confirm against the cache metrics rather than this table.

## Trace methods

`Filecoin.StateReplay`, `trace_block`, `trace_transaction`, `trace_filter`, `trace_replayBlockTransactions` and `debug_traceTransaction` re-execute an entire tipset with tracing enabled, which costs a few hundred milliseconds of CPU each. Two exceptions do not: `trace_call` applies a single synthetic message instead, and `debug_traceTransaction` with `prestateTracer` replays per message.

Start with `FOREST_TIPSET_TRACE_CACHE_SIZE`, which backs all of the re-executing methods, so a miss there is always a full re-execution. `FOREST_ETH_TRACE_BLOCK_CACHE_SIZE` sits in front of it for `trace_block`, `trace_transaction` and `trace_filter` only, and typically already runs above 90%, but its entries are far smaller, which makes it the cheaper of the two per MiB.

Check `cache_tipset_trace_hits_total` against `cache_tipset_trace_misses_total` afterwards. `cache_tipset_trace_size_bytes` shows what it is costing you, though that gauge is recomputed at most once every five minutes and can lag by that much.

## Ethereum block methods

`eth_getBlockByNumber` and `eth_getBlockByHash` build their response from a whole tipset and cache it keyed by tipset, so roughly one entry per epoch. `FOREST_ETH_BLOCK_CACHE_SIZE` defaults to 500, about four hours of chain, so a client walking a day's range evicts everything it needs before coming back to it. Raising it to 2000 covers roughly seventeen hours.

Check `cache_eth_block_full_tx_hits_total` against `cache_eth_block_full_tx_misses_total` afterwards. The setting covers the hashes-only form as well, and the memory figure above accounts for both.

## Message lookups

`FOREST_MESSAGES_IN_TIPSET_CACHE_SIZE` holds a tipset's decoded messages. The default of 8192 tipsets is about 2.8 days of chain.

This is the most traffic-dependent of the four, so measure before and after with `cache_msg_in_tipset_hits_total` and `cache_msg_in_tipset_misses_total`. Note the cache is not RPC-only: tipset execution, the message pool and gas estimation all read through it, so the hit rate moves with more than your clients.
