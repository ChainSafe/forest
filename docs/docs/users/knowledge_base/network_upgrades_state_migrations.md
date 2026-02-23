---
title: Network Upgrades & State Migrations
sidebar_position: 2
---

## Network upgrades

Network upgrades happen periodically in Filecoin. They bring new features to the network, proposed via [FIPs](https://github.com/filecoin-project/FIPs) and are announced via several channels, including the [filecoin-project/community discussions](https://github.com/filecoin-project/community/discussions) and the Filecoin Slack. Also, all Filecoin implementations make the announcements on their own. In Forest's case, it's on [Forest's Discussions](https://github.com/ChainSafe/forest/discussions) and the `#fil-forest-announcements` channel in the Filecoin Slack.

Some preparation is required for a smooth transition from one network version to another for node operators. Usually, this entails upgrading the node version only to a specified one before the upgrade - teams do their best to offer a final binary at least a week before the upgrade date.

## State migrations

State migration is part of the network upgrade. Given the size of the Filecoin state, it usually requires the node to go through every Actor, which takes several seconds. If more changes are required, the migration might take significantly more time and resources. The implementation teams announce the expected upgrade duration and requirements beforehand so that the node can be prepared accordingly.

:::tip
On average, ~3-4 network upgrades are performed annually. This number varies based on the FIPs proposed and implementer capacities. This means that the node shouldn't need all the resources that state migrations require. For example, during the NV22 network upgrade, Forest required 64 GiB memory. The following update needed at most 16 GiB memory. It may make sense to upgrade the node only around specific network upgrades.
:::

## Avoiding migrations / node recovery

Sometimes, it is not feasible to perform a network migration. If a node is hosted on a bare-metal server and not on a VPS, it might not be easy to have it upgraded. Fortunately, there is a way to avoid the painful migrations - Filecoin snapshots. The same applies when encountering an issue with your node (failing to follow the chain errors, consensus issues) - you should bootstrap the node from a fresh snapshot.

Forest snapshots are available in the [forest-archive](https://forest-archive.chainsafe.dev/list/) (they can also be used with Lotus). Latest snapshots for mainnet are offered by ChainSafe [here](https://forest-archive.chainsafe.dev/list/mainnet/latest). The link to the latest produced snapshot is [here](https://forest-archive.chainsafe.dev/latest/mainnet/). To avoid the network migration, stop it before the network upgrade and wait until a snapshot is generated **after** the upgrade.

:::info example
You read that in the [Forest NV23 support announcement](https://github.com/ChainSafe/forest/discussions/4488) the mainnet is going to be upgraded to NV23 at the epoch `4154640`, which corresponds to `2024-08-06T12:00:00Z`. You stop your note at least a minute before the upgrade, so before `2024-08-06T11:59:00Z` and wait until the latest snapshot at the [forest-archive](https://forest-archive.chainsafe.dev/latest/mainnet/) is newer than the epoch `4154640`.
You use `curl` to check the latest snapshot.

```bash
curl --no-progress-meter --head  https://forest-archive.chainsafe.dev/latest/mainnet/ | egrep 'height_(\d+)'
```

Sample output:

```console
location: /archive/mainnet/latest/forest_snapshot_mainnet_2024-08-06_height_415650.forest.car.zst
```

You see that the snapshot is past the upgrade epoch by ten epochs. You download the snapshot with the in-built tool which is faster than raw `cURL`.

```bash
forest-tool snapshot fetch --chain mainnet
```

You start your node with `--import-snapshot <snapshot-path>` and enjoy the new, fancy NV23 features. Hooray!

Alternatively, if you are fine with purging the current database, you can do it and use Forest's `--auto-download-snapshot` feature after confirming that the latest snapshot is past the upgrade epoch.

:::

:::warning
While the state migration can be avoided, this approach comes with significant downtime.

Depending on your network bandwidth, this can easily be over one hour after the upgrade occurs. The snapshot service needs to produce, then upload a snapshot, and only then users can fetch it. Given the snapshot size of over 70 GiB, this takes a non-negligible amount of time.
:::
