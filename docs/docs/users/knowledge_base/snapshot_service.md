---
title: Snapshot & Archival Data Service
---

# Snapshot service

## Latest snapshots

ChainSafe provides a snapshot service for the Filecoin network. The latest snapshots are generated approximately hourly and are available for the mainnet and calibration networks. The snapshots are stored on the [ChainSafe Filecoin Snapshot Service](https://forest-archive.chainsafe.dev/list). They store the last 2000 tipsets and are enough to bootstrap a new node. A checksum is provided for each snapshot to ensure its integrity.

:::info
The snapshots are compressed with the `zstd` algorithm. Both Forest and Lotus can read them, so there's no need for a manual decompression. On top of that, the snapshots include an index (hence the extension `.forest.car.zstd`) that allows them to be read-in place without importing it to a database (only Forest supports this feature). See the [Forest CAR format documentation](https://docs.rs/forest-filecoin/latest/forest_filecoin/db/car/forest/index.html) for more details. You might also want to watch [Filecoin Snapshots Explained](https://www.youtube.com/watch?v=GZ9VhCveRdA).
:::

## Archival snapshots

Archival snapshots are available free of charge. Note that they are not actively generated and are provided on a best-effort basis. Two types of archival snapshots are available:

- **Lite snapshots**: historical snapshots containing the last 2000 tipsets. They are available at 30,000 epoch intervals. Lite snapshots are useful for bootstrapping a node with historical data. They must be complemented with _diff_ snapshots for a complete historical chain.
- **Diff snapshots**: incomplete snapshots containing the new key-value pairs since the last diff snapshot.

# Snapshot generation details

The snapshot service is no longer open-source and not under Forest team's aegis. For past implementation, see the service [docker image](https://github.com/ChainSafe/forest-iac/tree/c928f5f9892cfd4b38ba718347ef28141dc667f9/images/snapshot-service) and the [Terraform module](https://github.com/ChainSafe/forest-iac/tree/c928f5f9892cfd4b38ba718347ef28141dc667f9/tf-managed/modules/daily-snapshot).

That said, the general algorithm for snapshot generation should stay the same. The service generates snapshots by:

1. Download the latest snapshot from the network (or use the last one it generated).
2. Import the snapshot into the node and wait for it to sync.
3. Export the snapshot.
4. Upload the snapshot.

# Snapshot generation performance

Forest produces snapshots compatible with Lotus, with much faster generation times and smaller memory usage. Below are the results of a recent comparison between Forest and Lotus snapshot generation.

This benchmark was performed on a bare metal server with the following specifications:

- `AMD EPYC 7F32 8-Core Processor`
- `512 GiB RAM`
- `4 TiB NVMe SSD`

|        | Generation time [min] | Avg. CPU usage [%] | Peak CPU usage [%] | Avg. memory usage [GiB] | Peak memory usage [GiB] |
| ------ | --------------------- | ------------------ | ------------------ | ----------------------- | ----------------------- |
| Forest | 28                    | 1.64               | 1.97               | 11                      | 14                      |
| Lotus  | 93                    | 6.15               | 10.6               | 122                     | 276                     |
