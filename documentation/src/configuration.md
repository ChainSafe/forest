# Configuration

The `forest` process has a set of configurable values which determine the behavior of the node. All values can be set through process flags or through a configuration file. If a configuration is provided through the flag and the configuration file, the flag value will be given preference.

## Flags

When starting `forest` you can configure the behavior of the process through the use of the following flags:

| Flag | Value | Description |
| ---- | ----- | ----------- |
| --config | OS File Path | Path to TOML file containing configuration |
| --genesis | OS File Path | CAR file with genesis state |
| --rpc | Boolean | Toggles the RPC API on |
| --port | Integer | Port for JSON-RPC communication |
| --token | String | Client JWT token to use for JSON-RPC authentication |
| --metrics-port | Integer | Port used for metrics collection server |
| --kademlia | Boolean | Determines whether Kademilia is allowed |
| --mdns | Boolean | Determines whether MDNS is allowed | 
| --import-snapshot | OS File Path | Path to snapshot CAR file |
| --import-chain | OS File Path | Path to chain CAR file |
| --skip-load | Boolean | Skips loading CAR File and uses header to index chain |
| --req-window | Integer | Sets the number of tipsets requested over chain exchange |
| --tipset-sample-size | Integer | Number of tipsets to include in the sample which determines the network head during synchronization |
| --target-peer-count | Integer | Amount of peers the node should maintain a connection with |
| --encrypt-keystore | Boolean | Controls whether the keystore is encrypted |

## Configuration File

Alternatively, when starting `forest` you can define a TOML configuration file and provide it to the process with the `--config` flag or through the `FOREST_CONFIG_PATH` environment variable.

The following is an sample configuration file: 

```toml
genesis = "/path/to/genesis/file"
rpc = true
port = 1234
token = "0394j3094jg0394jg34g"
metrics-port = 2345
kademlia = true
mdns = true
import-snapshot = /path/to/snapshot/file
import-chain = /path/to/chain/file
skip-load = false
req-window = 100
tipset-sample-size = 10
target-peer-count = 100
encrypt-keystore = false
```
