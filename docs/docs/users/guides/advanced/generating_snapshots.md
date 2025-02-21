---
title: Generating Snapshots
sidebar_position: 1
---

# Snapshot exporting 📸

## Hardware requirements

To export a mainnet snapshot, you need a setup with at least 16 GB of RAM. On a
machine with rapid NVMe, the default export should take around 30
minutes.

The requirements for calibnet snapshots are lower, but it is still recommended
to have at least 8 GB of RAM. The export should take less than a minute.

## Running the node

Wait until the node is fully synced. You can use the command:

```shell
forest-cli sync wait
```

## Exporting the snapshot

To export the snapshot with the defaults, run:

```shell
forest-cli snapshot export
```

The snapshot will be exported with 2000 recent stateroots to the current directory. The snapshot will be
compressed.

For mainnet, you should expect a file of over 70 GB. For calibnet, you should
expect a file of over 5 GB. Note that the snapshot size grows over time.

### CLI reference

Details on the `forest-cli snapshot export` command and its subcommands can be found at the [CLI reference](../../reference/cli#forest-cli-snapshot).
