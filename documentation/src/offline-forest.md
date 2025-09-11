# Offline Forest

Forest offers an offline mode, allowing using the snapshot as the source of
chain data and not connecting to any peers. This is useful for querying the
chain's archive state without syncing, and various testing scenarios.

## Usage

```bash
forest-tool api serve --help
```

Sample output (may vary depending on the version):

```console
Usage: forest-tool api serve [OPTIONS] [SNAPSHOT_FILES]...

Arguments:
  [SNAPSHOT_FILES]...  Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`

Options:
      --chain <CHAIN>           Filecoin network chain [default: mainnet]
      --port <PORT>             [default: 2345]
      --auto-download-snapshot
      --height <HEIGHT>         Validate snapshot at given EPOCH, use a negative value -N to validate the last N EPOCH(s) starting at HEAD [default: -50]
      --genesis <GENESIS>       Genesis file path, only applicable for devnet
  -h, --help                    Print help
```

## Example: serving a calibnet snapshot

The following command will start an offline server using the latest available
snapshot, which will be downloaded automatically. The server will listen on the
default port and act as a calibnet node _stuck_ at the latest snapshot's height.

```bash
forest-tool api serve --chain calibnet --auto-download-snapshot
```

## Example: serving a custom snapshot on calibnet

The following command will start an offline server using a custom snapshot,
which will be loaded from the provided path. The server will listen on the
default port and act as a calibnet node _stuck_ at the snapshot's
height: 1859736.

```bash
forest-tool api serve ~/Downloads/forest_snapshot_calibnet_2024-08-08_height_1859736.forest.car.zst
```

Sample output:

```console
2024-08-12T12:29:16.624698Z  INFO forest::tool::offline_server::server: Configuring Offline RPC Server
2024-08-12T12:29:16.640402Z  INFO forest::tool::offline_server::server: Using chain config for calibnet
2024-08-12T12:29:16.641654Z  INFO forest::genesis: Initialized genesis: bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4
2024-08-12T12:29:16.643263Z  INFO forest::daemon::db_util: Populating column EthMappings from range: [322354, 1859736]
...
2024-08-12T12:29:44.218675Z  INFO forest::tool::offline_server::server: Starting offline RPC Server
2024-08-12T12:29:44.218804Z  INFO forest::rpc: Ready for RPC connections
```

The server can then be queried using `forest-cli` or raw requests.

```bash
curl --silent -X POST -H "Content-Type: application/json" \
             --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
             "http://127.0.0.1:2345/rpc/v0" | jq
```

Sample output:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "Cids": [
      {
        "/": "bafy2bzaceafill2bfzjwfq7o5x5idqe2odrriz5n4pup5xfrbsdrzjsa6mspk"
      }
    ],
    "Blocks": [
      {
      ...
      }
    ],
    "Height": 1859736
  }
}
```

## Example: usage on a devnet

The devnet case is a bit more complex, as the genesis file and the network name
need to be provided. If no snapshots are provided, the server will start at the
genesis block.

```bash
forest-tool api serve --chain localnet-55c7758d-c91a-41eb-94a2-718cb4601bc5 --genesis /lotus_data/devgen.car
```

The server can be later queried:

```bash
curl --silent -X POST -H "Content-Type: application/json" \
             --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.StateGetNetworkParams","param":"null"}' \
             "http://127.0.0.1:2345/rpc/v0" | jq
```

Sample output:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "NetworkName": "localnet-55c7758d-c91a-41eb-94a2-718cb4601bc5",
    "BlockDelaySecs": 4,
    "ConsensusMinerMinPower": "2040",
    "SupportedProofTypes": [
      0,
      1
    ],
    "PreCommitChallengeDelay": 10,
    "ForkUpgradeParams": {
      "UpgradeSmokeHeight": -2,
      ...
      "UpgradeWaffleHeight": 18
    },
    "Eip155ChainID": 31415926
  }
}
```

Note that the network name will vary depending on the genesis file used.

⚠️ The offline server is unable to append blocks to the chain at the moment of
writing. See [#4598](https://github.com/ChainSafe/forest/issues/4598) for
details and updates. This means that starting the server only with a genesis
file won't be very useful, as the chain will be stuck at the genesis block.
