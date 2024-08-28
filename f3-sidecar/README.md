### EC tests

- run a forest node locally and expose RPC port at the default 2345
- run `go test -v .`

### Run sidecar

- run a forest node on calibnet
- import a shared miner key for testing `forest-wallet --remote-wallet import`
  (the shared miner worker key can be found in `scripts/tests/api_compare/.env`)
- run f3 sidecar `go run .`
- (optional) to inspect RPC calls, run
  `mitmproxy --mode reverse:http://localhost:2345 --listen-port 8080` then
  `go run . -rpc http://127.0.0.1:8080/rpc/v1`

### How F3 sidecar interacts with Forest

An F3 sidecar node is a standalone node that is part of a p2p network and
participates in the f3 protocol.

Besides what have been handled internally(e.g. p2p communications) in the
`go-f3` lib

- it uses `level-db` as storage backend
- it's bootstrapped with a manifest that contains parameters like bootstrapping
  epoch, chain finality and network name etc. the manifest can be constructed
  either statically, or dynamically by connecting to a p2p manifest server
- it requires an EC(expected consensus) backend to provide the chain information
  like chain head and power table etc
- it requires a signer backend to sign messages with the private keys of the
  participating miners
- it requires a backend that provides the actor IDs of the participating miners
- it requires a p2p node as bootstrapper to discover more peers via Kademlia
- additionally, to power the `Filecoin.F3*` RPC methods in forest, a sidecar
  node runs an RPC server that implements the same RPC methods to which the
  associated forest node can delegate the RPC requests

A brief diagram:

```mermaid
flowchart TD
    A[F3 sidecar] -->|EC API calls| B(Forest)
    A --> |signer API calls| B
    A --> |read manifest params| B
    A --> |P2P bootstrap node| B
    B --> |delegate F3 RPC calls| A
    A --> |storage backend| C[level db]
    A --> |dynamic manifest backend| D[manifest p2p server]
```
