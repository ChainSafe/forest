# Snapshot exporting 📸

## Hardware requirements

To export a mainnet snapshot, you need a setup with at least 10 GB of RAM. On a
machine with rapid NVMe SSD (around 7000MB/s), the export should take around 30
minutes.

The requirements for calibnet snapshots are lower, but it is still recommended
to have at least 4 GB of RAM. The export should take less than a minute.

## Running the node

You need to have a running node to be able to export a snapshot. If you don't
have one, you can follow the [usage guide](./basic_usage.md).

Wait until the node is fully synced. You can use the command:

```shell
forest-cli sync wait
```

## Exporting the snapshot

Usage of the ` snapshot export` command:

```shell
Usage: forest-cli snapshot export [OPTIONS]

Options:
  -o <OUTPUT_PATH>      Snapshot output filename or directory. Defaults to
                        `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`. [default: .]
      --skip-checksum   Skip creating the checksum file
      --dry-run         Don't write the archive
  -h, --help            Print help
```

The snapshot will be exported with 2000 recent stateroots.

To export the snapshot with the defaults, run:

```shell
forest-cli snapshot export
```

it will write the snapshot to the current directory. The snapshot will be
compressed.

For mainnet, you should expect a file of over 50 GB. For calibnet, you should
expect a file of around 1-2 GB.
