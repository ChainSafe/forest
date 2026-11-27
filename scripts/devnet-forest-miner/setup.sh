#!/bin/bash
# Bring up the Forest-sole devnet and wait for Forest's RPC to answer.

set -euxo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
pushd "${PARENT_PATH}"
set -a
# shellcheck source=/dev/null
source .env
# shellcheck source=/dev/null
source "${PARENT_PATH}/../devnet/upgrade_heights.env"
set +a
# shellcheck source=/dev/null
source "${PARENT_PATH}/../devnet/chain_height.sh"

# Start from a clean slate, but keep the `filecoin-proofs` volume so CI's cached proof params
# (and local re-runs) are reused rather than re-fetched.
docker compose down --remove-orphans
docker compose rm -f
docker volume rm -f devnet-forest-miner_lotus-data devnet-forest-miner_forest-data

# Run detached so we can probe it. `--wait` can't be used here: compose does not
# distinguish services from init containers. See https://github.com/docker/compose/issues/10596
docker compose up --build --force-recreate --detach

wait_forest_rpc

popd
