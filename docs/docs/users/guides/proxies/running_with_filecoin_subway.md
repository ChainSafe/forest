---
title: Running Forest With Filecoin Subway
sidebar_position: 3
---

## Filecoin Subway

[Filecoin Subway](https://github.com/protofire/filecoin-subway/tree/chain/filecoin) is a proxy between the Filecoin node and the client. It's forked from a proxy for Substrate and adapted to work with Filecoin.

:::note
For more information on configuring Filecoin Subway, refer to the [README](https://github.com/protofire/filecoin-subway/blob/chain/filecoin/README.md).
:::

### Configuring Filecoin Subway

Assuming you already have Forest running, there is not much configuration needed to run Filecoin Subway with Forest. A sample command is shown below (assuming you have installed Filecoin Subway system-wide):

```bash
UPSTREAM_URL=ws://localhost:2345/rpc/v1 RPC_PORT=2350 METRICS_PORT=6118 subway --config configs/eth_config.yml
```

where:

- `UPSTREAM_URL` is the WebSocket URL of your Forest node. Adjust the port if your Forest node is running on a different port.
- `RPC_PORT` is the port on which Filecoin Subway will listen for incoming RPC requests
- `METRICS_PORT` is the port on which Filecoin Subway will expose its metrics endpoint (ensure it doesn't clash with other services on your machine)
- `configs/eth_config.yml` is the configuration file for Filecoin Subway. You can modify it to suit your needs. This configuration file is included in the Filecoin Subway repository [here](https://github.com/protofire/filecoin-subway/blob/chain/filecoin/configs/eth_config.yml).

That's it! Forest should now be available only through Filecoin Subway. You can test it by running a sample command, e.g., subscribing to new blocks with [`websocat`](https://github.com/vi/websocat):

```console
websocat ws://127.0.0.1:2350/rpc/v1
{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newHeads"]}
{"jsonrpc":"2.0","result":"9HjqhIOCq6GpnTEa","id":1}
{"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"9HjqhIOCq6GpnTEa","result":{"baseFeePerGas":"0xda","difficulty":"0x0","extraData":"0x","gasLimit":"0x2540be400","gasUsed":...
```

There are more options and configurations available for Filecoin Subway; it's best to explore the repository for more details.
