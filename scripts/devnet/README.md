# Local devnet setup

The devnet consists of a:

- Lotus miner,
- Lotus node,
- Forest node.

It's packed in a docker compose setup for convenience and ease of usage. By
default, running it will expose relevant RPC and P2P ports to the host:

- 1234 - Lotus RPC,
- 9090 - Lotus P2P port,
- 2345 - Miner RPC,
- 3456 - Forest RPC.

## Running the devnet

Run it with:

```shell
docker compose up --build
```

This will build the local Forest (using the Dockerfile in the project's root)
image, tagged Lotus and setup the devnet. Initial setup may be slow due to
fetching params and setting everything up. Consecutive starts will be quick.

Stop the devnet with:

```shell
docker compose down
```

Remove the devnet:

```shell
docker compose rm
```

## Interacting with the devnet via CLI

Exec into the `forest` container:

```shell
docker exec -it forest /bin/bash
```

and setup credentials. Then run any command:

```shell
export TOKEN=$(cat /forest_data/token.jwt)
export FULLNODE_API_INFO=$TOKEN:/dns/forest/tcp/3456/http

forest-cli net peers
```

## Running the wallet integration tests

The same wallet/mpool integration suite that runs against calibnet can be run
against the local devnet. This brings up the devnet, waits for it to sync, wires
up the host environment, and runs the tests:

```shell
mise run test:wallet-devnet
```

Under the hood this sources `wallet_harness.sh`, which reads the admin token and
the funded genesis key from the running `forest` container, exports
`FULLNODE_API_INFO` (Forest RPC on port 3456) and `FOREST_TEST_PRELOADED_ADDRESS`,
then runs `forest-dev tests devnet mpool` and `forest-dev tests devnet wallet`.

## Local devnet development

If you prefer to have Forest running directly on the host, you can comment it
out and draw inspiration from the `docker-compose.yml` on how to connect it to
Lotus. In short, you will need to obtain the peer id, network name and the
genesis file.
