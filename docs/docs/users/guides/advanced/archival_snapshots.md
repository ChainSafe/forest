---
title: Archival Snapshots
sidebar_position: 2
---

# Archival Snapshots

ChainSafe hosts two kinds of snapshots: hourly snapshots (guaranteed to be no
more than a few hours old) and archival snapshots (similar to the regular
snapshots, but with less duplicate data). Archival snapshots come as 'lite'
snapshots, which include the entire block header history back to genesis, and
'diff' snapshots, which only contain the new data since the previous diff
snapshot.

Archival snapshots are publicly available here:

- Mainnet lites: https://forest-archive.chainsafe.dev/list/mainnet/lite
- Mainnet diffs: https://forest-archive.chainsafe.dev/list/mainnet/diff
- Calibnet lites: https://forest-archive.chainsafe.dev/list/calibnet/lite
- Calibnet diffs: https://forest-archive.chainsafe.dev/list/calibnet/diff

## Merging snapshots

Since 'diff' snapshots only contain the new data since the previous diff
snapshot, they need to be merged with the previous 'lite' snapshot to form a
complete snapshot. This can be done with the `forest-tool archive merge`
command.

```shell
forest-tool archive merge --output-file <output-file> <lite-snapshot> <diff-snapshots>
```

As an example, to get a snapshot that covers epoch 30_000 to epoch 36_000, you
merge `forest_snapshot_mainnet_2020-09-04_height_30000.forest.car.zst` with
`forest_diff_mainnet_2020-09-04_height_30000+3000.forest.car.zst` and
`forest_diff_mainnet_2020-09-05_height_33000+3000.forest.car.zst`.

## Generating archival snapshots

New archival snapshots can be generated either manually with `forest-tool
archive export` or automatically with `forest-tool archive sync-bucket`. Both
commands require a large snapshot file as input.

To generate archival snapshots manually, use these settings:

- one lite snapshot every 30_000 epochs,
- one diff snapshot every 3_000 epochs,
- a depth of 900 epochs for the diff snapshots,
- a depth of 900 for the lite snapshots.

Manual generation of archival snapshots should be a last resort. The
`forest-tool archive sync-bucket` command is recommended for generating
archival snapshots.
