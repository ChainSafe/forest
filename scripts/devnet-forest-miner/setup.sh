#!/bin/bash
# Bring up the Forest-sole devnet and wait for Forest's RPC to answer.

set -euxo pipefail

# Path to the directory containing this script.
PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
pushd "${PARENT_PATH}"
source .env

# Start from a clean slate, but keep the `filecoin-proofs` volume so CI's cached proof params
# (and local re-runs) are reused rather than re-fetched.
docker compose down --remove-orphans
docker compose rm -f
docker volume rm -f devnet-forest-miner_lotus-data devnet-forest-miner_forest-data

# Run detached so we can probe it. `--wait` can't be used here: compose does not
# distinguish services from init containers. See https://github.com/docker/compose/issues/10596
docker compose up --build --force-recreate --detach

function call_forest_chain_head {
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
       "http://127.0.0.1:${FOREST_RPC_PORT}/rpc/v1"
}

until call_forest_chain_head; do
  echo "Forest is unavailable - sleeping for 1s"
  sleep 1
done

popd
