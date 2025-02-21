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
especially when backing up the database**

`forest-tool backup create` will create a backup file in the current working
directory. It will contain the p2p key-pair used to derive the `PeerId` and the
keystore. If storing anywhere, make sure to encrypt it.

```shell
forest-tool backup create
```

Sample output:

```console
Adding /home/rumcajs/.local/share/forest/libp2p/keypair to backup
Adding /home/rumcajs/.local/share/forest/keystore.json to backup
Backup complete: forest-backup-2024-02-22_17-18-43.tar
```

Afterwards, you can use `forest-tool backup restore <backup-file>` to restore
those files. Note that this assumes that Forest is using the default
configuration - if it's not the case, provide the configuration `TOML` file via
the `--daemon-config` parameter.

```shell
forest-tool backup restore forest-backup-2024-02-22_17-18-43.tar
```

Sample output:

```console
Restoring /home/rumcajs/.local/share/forest/libp2p/keypair
Restoring /home/rumcajs/.local/share/forest/keystore.json
Restore complete
```

There are other flags to the backup tool, most notably `--all`, that will back
up the entire Forest data directory. Note that this includes the whole
blockstore, which, for mainnet, can reach hundreds of gigabytes. It is not
recommended outside development.

### CLI reference

Details on the `forest-tool backup` command and its subcommands can be found at the [CLI reference](../../reference/cli#forest-tool-backup).
