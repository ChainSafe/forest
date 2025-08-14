# RPC Compatibility

A running Lotus node can be accessed through an RPC interface. The RPC methods
are listed here:

- V0 methods: [Lotus V0 API methods (deprecated)](https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v0-deprecated.md)
- V1 methods: [Lotus V1 API methods (stable)](https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-methods-v1-stable.md)

The current status of compatibility can be checked by comparing a running Forest
node with a running Lotus node:

1. Build Lotus with support for Calibnet and sync to HEAD. Run Lotus with
   `LOTUS_FEVM_ENABLEETHRPC=1` to enable the Eth RPC methods.
2. Run Forest against Calibnet and sync to HEAD.
3. Run `forest-tool api compare`

The output will look like this:

| RPC Method                        | Forest              | Lotus         |
| --------------------------------- | ------------------- | ------------- |
| Filecoin.ChainGetBlock            | Valid               | Valid         |
| Filecoin.ChainGetGenesis          | Valid               | Valid         |
| Filecoin.ChainGetMessage (67)     | InternalServerError | Valid         |
| Filecoin.ChainGetMessagesInTipset | MissingMethod       | Valid         |
| Filecoin.ChainGetTipSetByHeight   | Valid               | Valid         |
| Filecoin.ChainHead                | Valid               | Valid         |
| Filecoin.ChainReadObj             | InvalidResponse     | Valid         |
| Filecoin.Discover                 | MissingMethod       | Valid         |
| Filecoin.MpoolPending             | Valid               | Valid         |
| Filecoin.NetAddrsListen           | Valid               | Valid         |
| Filecoin.NetInfo                  | Valid               | MissingMethod |
| Filecoin.NetPeers                 | Valid               | Valid         |
| Filecoin.Session                  | MissingMethod       | Valid         |
| Filecoin.StartTime                | Valid               | Valid         |
| Filecoin.StateGetActor            | InternalServerError | Valid         |
| Filecoin.StateMinerPower (76)     | MissingMethod       | Valid         |
| Filecoin.StateNetworkName         | Valid               | Valid         |
| Filecoin.Version                  | Valid               | Valid         |

If an entry for Lotus is not marked as `Valid`, this indicates that the Forest
RPC client is buggy and incorrectly communicates with Lotus.

## Limitations

Forest aims at being a drop-in replacement for Lotus and have support for all of
the RPC methods. Note, some methods (like `Filecoin.ChainHotGC`) are
Lotus-specific and are meaningless in Forest. Such methods should be no-ops in
Forest.

Forest does not yet support mining and none of the mining-related RPC calls will
be implemented in the foreseeable future.

## Gateway

The `lotus-gateway` executable is a reverse-proxy that sanitizes RPC calls
before they're forwarded to a Filecoin node. The `forest-tool api compare`
command will fail if run against a gateway rather than directly against a node.
This means API compatiblity testing has to be done with a local node rather than
`api.node.glif.io`.

## Use `mitmproxy`

Inspecting RPC calls is best done with a reverse proxy. If Lotus listens to port
1234 and Forest listens to port 2345, run the API compatibility tests through
reverse proxies:

1. `mitmproxy --mode reverse:http://localhost:2345 --listen-port 8080`
2. `mitmproxy --mode reverse:http://localhost:1234 --listen-port 8081`
3. `forest-tool api compare --forest /ip4/127.0.0.1/tcp/8080/http --lotus /ip4/127.0.0.1/tcp/8081/http`

Request / Response pairs will show up in the `mitmproxy` windows.

## Adding a new method

Checklist for adding a new RPC method:

1. Add method name in `src/rpc_api/mod.rs` and set the access level.
2. Add request/response data types to `src/rpc_api/data_types.rs` as needed.
3. Add `RpcRequest` in the appropriate file in `src/rpc_client/`.
4. Test the method in `src/tool/subcommands/api_cmd.rs`. The method should show
   up as `Valid` for Lotus and `MissingMethod` for Forest. Use `mitmproxy` to
   debug.
5. Implement Forest endpoint in `src/rpc/`, add it to the method list in
   `src/rpc/mod.rs`
6. Verify that the test from step 4 shows `Valid` for Forest.

## Creating own miner for tests

Use commands along the lines of the following script to create a miner for
testing. Note that the `miner create` will take a while to complete.

```bash
#!/bin/bash
# Owner
# The owner keypair is provided by the miner ahead of registration and its public key associated with the miner address.
# The owner keypair can be used to administer a miner and withdraw funds.
OWNER=$(lotus wallet new bls)
WORKER=$(lotus wallet new bls)
SENDER=$(lotus wallet new bls)

# print the owner address and order the user to send FIL from faucet to it. Wait for the confirmation from the user.
echo "Owner: $OWNER"
echo "Please send some FIL to the owner address and press enter to continue. Ensure that that the transaction is confirmed."
read

# Send some FIL to the worker and sender from the owner address
lotus send --from $OWNER $WORKER 10
lotus send --from $OWNER $SENDER 10

echo "Wait till the funds are confirmed and press enter to continue."
read

lotus-shed miner create $SENDER $OWNER $WORKER 32GiB
```

Afterwards, use the `lotus wallet export` and `lotus wallet import` commands to
persist and restore the keys.
