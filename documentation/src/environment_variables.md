# Environment variables

Besides CLI options and the configuration values in the configuration file,
there are some environment variables that control the behaviour of a `forest`
process.

| Environment variable       | Value                            | Default | Description                                              |
| -------------------------- | -------------------------------- | ------- | -------------------------------------------------------- |
| FOREST_KEYSTORE_PHRASE_ENV | any text                         | empty   | The passphrase for the encrypted keystore                |
| FOREST_CAR_LOADER_FILE_IO  | 1 or true                        | false   | Load CAR files with `RandomAccessFile` instead of `Mmap` |
| FOREST_DB_DEV_MODE         | [see here](#-forest_db_dev_mode) | current | The database to use in development mode                  |

### FOREST_DB_DEV_MODE

By default, Forest will create a database of its current version or try to
migrate to it. This can be overridden with the `FOREST_DB_DEV_MODE`
environmental variable.

| Value                          | Description                                                                                                                                      |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `current` or (unset)           | Forest will either create a new database with the current version or attempt a migration if possible. On failure, it will create a new database. |
| `latest`                       | Forest will use the latest versioned database. No migration will be performed.                                                                   |
| other values (e.g., `cthulhu`) | Forest will use the provided database (if it exists, otherwise it will create one under this name)                                               |

The databases can be found, by default, under `<DATA_DIR>/<chain>/`, e.g.,
`$HOME/.local/share/forest/calibnet`.
