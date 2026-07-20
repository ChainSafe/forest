---
sidebar_position: 3
title: Snapshot Generation Comparison
---

# Snapshot Generation: Forest vs Lotus

Forest plays an important role in timely, efficient network snapshot generation: while it is possible to generate a snapshot with Lotus, doing so is significantly slower and more expensive. On a regular machine, Forest completes a basic chain snapshot export much faster than Lotus, and with a fraction of the memory.

<p align="center">
  <img
    src="/img/reports/snapshot_comparison/snapshot-export-comparison.png"
    alt="Snapshot export duration: Forest vs Lotus"
    style={{ maxWidth: '480px', width: '100%' }}
  />
</p>

Both implementations produce compatible snapshots and can consume snapshots produced by the other.

:::note
Lotus snapshots are not compressed by default; Forest compresses them as part of the export. Both implementations can consume compressed snapshots. Compressing a Lotus snapshot after generation is possible but adds further to the total time.

Forest snapshots (`.forest.car.zst`) also embed an index that lets Forest use the file directly as a read-only blockstore, serving blocks in place without importing them into a database. The index is stored in skippable frames, so Lotus and other tools can still read the snapshot; they just ignore the index. Reading a snapshot in place this way is currently Forest-only.
:::

## Related

- [RPC Performance Comparison](./rpc_comparison.md)
- [Forest vs Lotus feature comparison](./index.md)
