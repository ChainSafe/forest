# Forest Backups

> "_The condition of any backup is unknown until a restore is attempted._"
> Everyone who deals with backups.

## Manual backups

The manual way requires knowledge of Forest internals and how it structures its
data directory (which is not guaranteed to stay the same). Thus, it is
recommended to use alternatives.

## Backups with the `forest-tool`

Forest comes with a `forest-tool` binary, which handles creating and recovering
backups.

### Basic usage

:warning: **The Forest node should be offline during the backup process,
especially when backing up the blockstore.**

`forest-tool backup create` will create a backup file in the current working
directory. It will contain the p2p keypair used to derive the `PeerId` and the
keystore. If storing anywhere, make sure to encrypt it.

```
❯ forest-tool backup create
Adding /home/rumcajs/.local/share/forest/libp2p/keypair to backup
Adding /home/rumcajs/.local/share/forest/keystore.json to backup
Backup complete: forest-backup-2024-02-22_17-18-43.tar
```

Afterwards, you can use `forest-tool backup restore <backup-file>` to restore
those files. Note that this assumes that Forest is using the default
configuration - if it's not the case, provide the configuration TOML file via
the `--daemon-config` parameter.

```
❯ forest-tool backup restore forest-backup-2024-02-22_17-18-43.tar
Restoring /home/rumcajs/.local/share/forest/libp2p/keypair
Restoring /home/rumcajs/.local/share/forest/keystore.json
Restore complete
```

There are other flags to the backup tool, most notably `--all`, that will back
up the entire Forest data directory. Note that this includes the whole
blockstore, which, for mainnet, can reach hundreds of gigabytes. It is not
recommended outside development.

### `backup`

```
Create and restore backups

Usage: forest-tool backup <COMMAND>

Commands:
  create   Create a backup of the node. By default, only the p2p keypair and keystore are backed up. The node must be offline
  restore  Restore a backup of the node from a file. The node must be offline
  help     Print this message or the help of the given subcommand(s)
```

### `backup create`

```
Create a backup of the node. By default, only the p2p keypair and keystore are backed up. The node must be offline

Usage: forest-tool backup create [OPTIONS]

Options:
      --backup-file <BACKUP_FILE>      Path to the output backup file if not using the default
      --all                            Backup everything from the Forest data directory. This will override other options
      --no-keypair                     Disables backing up the keypair
      --no-keystore                    Disables backing up the keystore
      --backup-chain <BACKUP_CHAIN>    Backs up the blockstore for the specified chain. If not provided, it will not be backed up
      --include-proof-params           Include proof parameters in the backup
  -d, --daemon-config <DAEMON_CONFIG>  Optional TOML file containing forest daemon configuration. If not provided, the default configuration will be used
  -h, --help                           Print help
```

### `backup restore`

```
Restore a backup of the node from a file. The node must be offline

Usage: forest-tool backup restore [OPTIONS] <BACKUP_FILE>

Arguments:
  <BACKUP_FILE>  Path to the backup file

Options:
  -d, --daemon-config <DAEMON_CONFIG>  Optional TOML file containing forest daemon configuration. If not provided, the default configuration will be used
      --force                          Force restore even if files already exist WARNING: This will overwrite existing files
  -h, --help                           Print help
```
