---
title: Garbage Collector
sidebar_position: 5
---

### Enabling/Disabling Automatic Garbage Collection

By default, automatic garbage collection is enabled in Forest to ensure that unnecessary data is regularly cleared out, optimizing disk usage and performance. The default GC interval is 20160 epochs(7 days). The interval can be overridden by setting environment variable `FOREST_SNAPSHOT_GC_INTERVAL_EPOCHS`.
Note that, an extra random small delay is added to the GC interval on every GC cycle to avoid a cluster of nodes run GC and reboot RPC services at the same time.

If you want to disable the automatic GC, for example, while testing new features or running performance benchmarks where GC may cause unnecessary overhead, you can do so by starting the Forest daemon with the `--no-gc` flag.

This ensures that GC scheduler is disabled, preventing potential performance impact.

### Manually Run Garbage Collection

GC can be trigger manually with `forest-cli chain prune snap`, regardless whether GC scheduler is enabled or disabled. Note that there is a global lock that ensures only one GC task could be running.

### Cadence of GC Runs

Garbage Collection (GC) runs on a regular schedule and follows these steps:

- Export an effective standard lite snapshot in `.forest.car.zst` format.
- Stop the node.
- Purge parity-db columns that serve as non-persistent blockstore.
- Purge old CAR database files.
- Restart the node.

This process keeps the system clean by regularly removing old, unused data.

### What is Garbage Collected?

The GC process removes unreachable blocks and state trees that are older than chain finality epochs. Specifically, it:

- Removes state trees that are no longer needed to ensure the persistence layer remains lightweight and efficient.
- Removes stale lite snapshots in the CAR database.

GC does not remove:

- Persistent blockstore column.
- Settings column.
- Ethereum index column

### When Should You Disable GC?

GC is critical in production environments, but you may want to disable it in the following cases:

- **Performance benchmarking**: When you want to measure raw performance without GC overhead.
- **Testing new features**: When developing or experimenting with features where GC pauses might interfere with quick iteration cycles.

Always remember to enable GC when moving back to production or long-term testing environments.

## What Happens During a GC Run?

### RAM/Disk Usage Spikes

During the GC process, Forest consumes extra RAM and disk space temporarily:

- While traversing reachable blocks, it uses 32 bytes of RAM per reachable block.
- While exporting a lite snapshot, it uses extra disk space before cleaning up parity-db and stale CAR snapshots.

For a typical ~80 GiB mainnet snapshot, this results in ~2.5 GiB of additional RAM and ~80 GiB disk space usage.

### Syncing Pauses or Performance Overheads

While GC runs in the background, it can cause some delays or pauses, particularly during the "export" stage, where reachable blocks are processed:

- **Syncing Pauses**: There may be brief interruptions in syncing as resources are allocated for the GC process.
- **Performance Overhead**: While relatively efficient, the chain traversal algorithm could slow down operations slightly.
- **Reboot pauses**: The GC stops the node before cleaning up parity-db and CAR snapshots and then restarts the node, which could take `~10s-~30s` on mainnet

## Disk Usage

### With GC

When GC is running, it consumes extra disk space of the size of a standard lite snapshot:

### Without GC

If GC is disabled, the database will continue to grow as unreachable data remains in the system. Over time, this can lead to significant disk space consumption, especially in active environments where many blocks are added, and forks may create unreachable blocks.

Without GC, expect disk usage to grow without bounds as old data is never purged. This could lead to:

- Higher storage costs.
- Decreased performance as the database grows larger and becomes more fragmented.

For detailed information on the inner workings of the GC, refer to the [GC documentation](https://docs.rs/forest-filecoin/latest/forest/db/gc/index.html)
