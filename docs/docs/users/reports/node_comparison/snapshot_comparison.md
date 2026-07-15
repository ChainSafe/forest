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

Forest `0.33.6` finished the export in 34 minutes using 32 GiB of RAM. Lotus `1.36.0` took 450 minutes (about `13x` slower) with 128 GiB, and 232 minutes (about `7x` slower) even with 256 GiB. Forest also needs less than half the disk space for the resulting snapshot. The table below compares a basic snapshot export across implementations.

|        | Required disk space [GiB] | RAM [GiB] | Export duration [minutes] |
| ------ | ------------------------- | --------- | ------------------------- |
| Forest | 200                       | 32        | 34                        |
| Lotus  | 450                       | 128       | 450                       |
| Lotus  | 450                       | 256       | 232                       |

Both implementations produce compatible snapshots and can consume snapshots produced by the other.

:::note
Lotus snapshots are not compressed by default; Forest compresses them as part of the export. Both implementations can consume compressed snapshots. Compressing a Lotus snapshot after generation is possible but adds further to the total time.
:::

## Related

- [RPC Performance Comparison](./rpc_comparison.md)
- [Forest vs Lotus feature comparison](./index.md)
