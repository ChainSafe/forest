---
title: Garbage Collector
sidebar_position: 5
---

### Enabling/Disabling Garbage Collection

By default, garbage collection is automatically enabled in Forest to ensure that unnecessary data is regularly cleared out, optimizing disk usage and performance.

If you want to disable GC, for example, while testing new features or running performance benchmarks where GC may cause unnecessary overhead, you can do so by starting the Forest daemon with the `--no-gc` flag.

This ensures that GC process is skipped, preventing potential performance impact.

### Cadence of GC Runs

Garbage Collection (GC) runs on a regular schedule and follows these steps:

- **Mark**: Scan all the data blocks, create a unique identifier (hash) for each, and store these in a list.
- Wait for the chain to reach finality
- **Sweep**: Check which data is still needed by starting at the latest block. Anything not needed and older than the finality point gets deleted.

This process keeps the system clean by regularly removing old, unused data.

### What is Garbage Collected?

The GC process removes unreachable blocks and state trees that are older than chain finality epochs. Specifically, it:

- Marks blocks that can no longer be reached from the blockchain’s HEAD and prepares them for deletion.
- Removes state trees that are no longer needed to ensure the persistence layer remains lightweight and efficient.

GC does not remove:

- Any block that is reachable from the current heaviest tipset.
- Data younger than chain finality epochs (to prevent the accidental removal of blocks that may later be part of a fork).

### When Should You Disable GC?

GC is critical in production environments, but you may want to disable it in the following cases:

- **Performance benchmarking**: When you want to measure raw performance without GC overhead.
- **Testing new features**: When developing or experimenting with features where GC pauses might interfere with quick iteration cycles.

Always remember to enable GC when moving back to production or long-term testing environments.

## What Happens During a GC Run?

### Memory Usage Spikes

During the GC process, Forest consumes extra memory temporarily:

- **Mark Phase**: It requires 4 bytes of memory per database record.
- **Filter Phase**: While traversing reachable blocks, it uses 32 bytes of memory per reachable block.

For a typical 100 GiB mainnet snapshot, this results in approximately 2.5 GiB of additional memory usage.

### Syncing Pauses or Performance Overheads

While GC runs in the background, it can cause some delays or pauses, particularly during the "filter" stage, where reachable blocks are processed:

- **Syncing Pauses**: There may be brief interruptions in syncing as resources are allocated for the GC process.
- **Performance Overhead**: While relatively efficient, the mark-and-sweep algorithm could slow down operations slightly, especially on large datasets.

## Disk Usage

### With GC

When GC is enabled, you can expect disk usage to be slightly higher than live data for three reasons:

1. Unreachable data isn’t removed until it’s at least 7.5 hours old.
2. The GC is conservative, leaving behind a small amount of unreachable data (less than 1%).
3. Some fragmentation in the blockstore backend may prevent immediate disk space reclamation.

Overall, the disk usage is expected to be slightly above the live dataset size, which helps maintain optimal database performance.

### Without GC

If you disable GC, the database will continue to grow as unreachable data remains in the system. Over time, this can lead to significant disk space consumption, especially in active environments where many blocks are added, and forks may create unreachable blocks.

Without GC, expect disk usage to grow without bounds as old data is never purged. This could lead to:

- Higher storage costs.
- Decreased performance as the database grows larger and becomes more fragmented.

For detailed information on the inner workings of the GC, refer to the [GC documentation](https://docs.rs/forest-filecoin/0.20.0/forest_filecoin/db/gc/index.html)
