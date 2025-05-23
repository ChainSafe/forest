# Local devnet setup

The devnet consists of a:

- Lotus miner,
- Lotus node,
- Forest node.
- Curio node.
- YugabyteDB.

It's packed in a docker compose setup for convenience and ease of usage. By
default, running it will expose relevant RPC and P2P ports to the host:

- 1234 - Lotus RPC,
- 9090 - Lotus P2P port,
- 2345 - Miner RPC,
- 3456 - Forest RPC.
- 12300 - Curio API.
- 4701 - Curio UI.
- 32100 - Curio Market.

## Running the devnet

Run it with:

```shell
docker compose up --build
```

This will build the local Forest (using the Dockerfile in the project's root)
image, tagged Lotus and setup the devnet. Initial setup may be slow due to
fetching params and setting everyting up. Consecutive starts will be quick.

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
export FULLNODE_API_INFO=$TOKEN:/dns/forest/tcp/1234/http

forest-cli net peers
```

## Known problems

- YugabyteDB sometimes fails to start. If this happens, restart the devnet.

## Local devnet development

If you prefer to have Forest running directly on the host, you can comment it
out and draw inspiration from the `docker-compose.yml` on how to connect it to
Lotus. In short, you will need to obtain the peer id, network name and the
genesis file.

## Testing mining

By default, Curio won't mine any sectors. You can add sectors by starting a new
container:

```
docker exec -it curio /bin/bash
```

And then run:

```
curio seal start --now --cc --actor t01002
```

You can inspect the status at `localhost:4701`. After a few hours, the sectors
should be sealed and proven.
