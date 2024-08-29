# Environment variables

Besides CLI options and the configuration values in the configuration file,
there are some environment variables that control the behaviour of a `forest`
process.

| Environment variable                                    | Value                            | Default                          | Description                                                                      |
| ------------------------------------------------------- | -------------------------------- | -------------------------------- | -------------------------------------------------------------------------------- |
| FOREST_KEYSTORE_PHRASE                                  | any text                         | empty                            | The passphrase for the encrypted keystore                                        |
| FOREST_CAR_LOADER_FILE_IO                               | 1 or true                        | false                            | Load CAR files with `RandomAccessFile` instead of `Mmap`                         |
| FOREST_DB_DEV_MODE                                      | [see here](#-forest_db_dev_mode) | current                          | The database to use in development mode                                          |
| FOREST_ACTOR_BUNDLE_PATH                                | file path                        | empty                            | Path to the local actor bundle, download from remote servers when not set        |
| FIL_PROOFS_PARAMETER_CACHE                              | dir path                         | empty                            | Path to folder that caches fil proof parameter files                             |
| FOREST_PROOFS_ONLY_IPFS_GATEWAY                         | 1 or true                        | false                            | Use only IPFS gateway for proofs parameters download                             |
| FOREST_FORCE_TRUST_PARAMS                               | 1 or true                        | false                            | Trust the parameters downloaded from the Cloudflare/IPFS                         |
| IPFS_GATEWAY                                            | URL                              | https://proofs.filecoin.io/ipfs/ | The IPFS gateway to use for downloading proofs parameters                        |
| FOREST_RPC_DEFAULT_TIMEOUT                              | Duration (in seconds)            | 60                               | The default timeout for RPC calls                                                |
| FOREST_MAX_CONCURRENT_REQUEST_RESPONSE_STREAMS_PER_PEER | positive integer                 | 10                               | the maximum concurrent streams per peer for request-response-based p2p protocols |
| FOREST_BLOCK_DELAY_SECS                                 | positive integer                 | Depends on the network           | Duration of each tipset epoch                                                    |
| FOREST_PROPAGATION_DELAY_SECS                           | positive integer                 | Depends on the network           | How long to wait for a block to propagate through the network                    |
| FOREST_MAX_FILTERS                                      | integer                          | 100                              | The maximum number of filters                                                    |
| FOREST_MAX_FILTER_RESULTS                               | integer                          | 10,000                           | The maximum number of filter results                                             |
| FOREST_MAX_FILTER_HEIGHT_RANGE                          | integer                          | 2880                             | The maximum filter height range allowed, a conservative limit of one day         |

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
