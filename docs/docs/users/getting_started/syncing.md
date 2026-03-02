---
title: Syncing A Node
sidebar_position: 3
---

:::info

All nodes joining the network are recommended to sync from a snapshot. This is the default behavior of Forest.

Syncing from genesis (tipset 0) is generally infeasible.

:::

Once started, Forest will connect to the bootstrap peers and in parallel fetch the latest snapshot from [Forest's snapshot service](../knowledge_base/snapshot_service). Once the snapshot is downloaded, it will be loaded into the node, and then syncing will continue by utilizing its peers.

### Mainnet

```shell
forest
```

### Calibnet

```shell
forest --chain calibnet
```

## Monitoring Sync Status

In another shell:

```shell
forest-cli sync status
```
