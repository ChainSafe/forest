---
title: Environment Variables
sidebar_position: 2
---

# Environment variables

Besides CLI options and the configuration values in the configuration file,
there are some environment variables that control the behavior of a `forest`
process.

| Environment variable                                      | Value                            | Default                                        | Example                                                       | Description                                                                      |
| --------------------------------------------------------- | -------------------------------- | ---------------------------------------------- | ------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `FOREST_KEYSTORE_PHRASE`                                  | any text                         | empty                                          | `asfvdda`                                                     | The passphrase for the encrypted keystore                                        |
| `FOREST_CAR_LOADER_FILE_IO`                               | 1 or true                        | false                                          | true                                                          | Load CAR files with `RandomAccessFile` instead of `Mmap`                         |
| `FOREST_DB_DEV_MODE`                                      | [see here](#-forest_db_dev_mode) | current                                        | current                                                       | The database to use in development mode                                          |
| `FOREST_ACTOR_BUNDLE_PATH`                                | file path                        | empty                                          | `/path/to/file.car.zst`                                       | Path to the local actor bundle, download from remote servers when not set        |
| `FIL_PROOFS_PARAMETER_CACHE`                              | directory path                   | empty                                          | `/var/tmp/filecoin-proof-parameters`                          | Path to folder that caches fil proof parameter files                             |
| `FOREST_PROOFS_ONLY_IPFS_GATEWAY`                         | 1 or true                        | false                                          | 1                                                             | Use only IPFS gateway for proofs parameters download                             |
| `FOREST_FORCE_TRUST_PARAMS`                               | 1 or true                        | false                                          | 1                                                             | Trust the parameters downloaded from the Cloudflare/IPFS                         |
| `IPFS_GATEWAY`                                            | URL                              | `https://proofs.filecoin.io/ipfs/`             | `https://proofs.filecoin.io/ipfs/`                            | The IPFS gateway to use for downloading proofs parameters                        |
| `FOREST_RPC_DEFAULT_TIMEOUT`                              | Duration (in seconds)            | 60                                             | 10                                                            | The default timeout for RPC calls                                                |
| `FOREST_MAX_CONCURRENT_REQUEST_RESPONSE_STREAMS_PER_PEER` | positive integer                 | 10                                             | 10                                                            | the maximum concurrent streams per peer for request-response-based p2p protocols |
| `FOREST_BLOCK_DELAY_SECS`                                 | positive integer                 | Depends on the network                         | 30                                                            | Duration of each tipset epoch                                                    |
| `FOREST_PROPAGATION_DELAY_SECS`                           | positive integer                 | Depends on the network                         | 20                                                            | How long to wait for a block to propagate through the network                    |
| `FOREST_MAX_FILTERS`                                      | integer                          | 100                                            | 100                                                           | The maximum number of filters                                                    |
| `FOREST_MAX_FILTER_RESULTS`                               | positive integer                 | 10,000                                         | 10000                                                         | The maximum number of filter results                                             |
| `FOREST_MAX_FILTER_HEIGHT_RANGE`                          | positive integer                 | 2880                                           | 2880                                                          | The maximum filter height range allowed, a conservative limit of one day         |
| `FOREST_STATE_MIGRATION_THREADS`                          | integer                          | Depends on the machine.                        | 3                                                             | The number of threads for state migration thread-pool. Advanced users only.      |
| `FOREST_CONFIG_PATH`                                      | string                           | /$FOREST_HOME/com.ChainSafe.Forest/config.toml | `/path/to/config.toml`                                        | Forest configuration path. Alternatively supplied via `--config` cli parameter.  |
| `RUST_LOG`                                                | string                           | empty                                          | `debug,forest_libp2p::service=info`                           | Allows for log level customization.                                              |
| `FOREST_F3_SIDECAR_RPC_ENDPOINT`                          | string                           | 127.0.0.1:23456                                | `127.0.0.1:23456`                                             | An RPC endpoint of F3 sidecar.                                                   |
| `FOREST_F3_SIDECAR_FFI_ENABLED`                           | 1 or true                        | hard-coded per chain                           | 1                                                             | Whether or not to start the F3 sidecar via FFI                                   |
| `FOREST_F3_CONSENSUS_ENABLED`                             | 1 or true                        | hard-coded per chain                           | 1                                                             | Whether or not to apply the F3 consensus to the node                             |
| `FOREST_F3_FINALITY`                                      | integer                          | inherited from chain configuration             | 900                                                           | Set the chain finality epochs in F3 manifest                                     |
| `FOREST_F3_PERMANENT_PARTICIPATING_MINER_ADDRESSES`       | comma delimited strings          | empty                                          | `t0100,t0101`                                                 | Set the miner addresses that participate in F3 permanently                       |
| `FOREST_F3_INITIAL_POWER_TABLE`                           | string                           | empty                                          | `bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i` | Set the F3 initial power table CID                                               |
| `FOREST_F3_ROOT`                                          | string                           | [FOREST_DATA_ROOT]/f3                          | `/var/tmp/f3`                                                 | Set the data directory for F3                                                    |
| `FOREST_F3_BOOTSTRAP_EPOCH`                               | integer                          | -1                                             | 100                                                           | Set the bootstrap epoch for F3                                                   |
| `FOREST_F3_MANIFEST_POLL_INTERVAL`                        | string                           | empty                                          | `15m`                                                         | Set the contract manifest poll interval for F3                                   |
| `FOREST_DRAND_MAINNET_CONFIG`                             | string                           | empty                                          | refer to Drand config format section                          | Override `DRAND_MAINNET` config                                                  |
| `FOREST_DRAND_QUICKNET_CONFIG`                            | string                           | empty                                          | refer to Drand config format section                          | Override `DRAND_QUICKNET` config                                                 |

### `FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT`

This is an environment variable that allows users to opt out of building the f3-sidecar. It's only useful when building
the binary.

By default, the Go f3-sidecar is built and linked into Forest binary unless environment
variable `FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT=1` is set.

### `FOREST_DB_DEV_MODE`

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

### Drand config format

```json
{
  "servers": ["https://api.drand.sh/"],
  "chain_info": {
    "public_key": "83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a",
    "period": 3,
    "genesis_time": 1692803367,
    "hash": "52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971",
    "groupHash": "f477d5c89f21a17c863a7f937c6a6d15859414d2be09cd448d4279af331c5d3e"
  },
  "network_type": "Quicknet"
}
```
