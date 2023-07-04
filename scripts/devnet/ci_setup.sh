#!/bin/bash
# This script is used to set up the CI environment for the
# local devnet tests.

set -euxo pipefail

# Path to the directory containing this script.
PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
pushd "${PARENT_PATH}"
source .env

# We use a custom Dockerfile for CI to speed up the build process.
export FOREST_DOCKERFILE_OVERRIDE=scripts/devnet/forest_ci.dockerfile

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans
docker compose rm -f

# Run it in the background so we can perform checks on it.
# Ideally, we could use `--wait` and `--wait-timeout` to wait for services
# to be up. However, `compose` does not distinct between services and 
# init containers. See more: https://github.com/docker/compose/issues/10596
docker compose up --build --force-recreate --detach

# Wait for Forest to be ready. We can assume that it is ready when the
# RPC server is up. This checks if Forest's RPC endpoint is up.
 function call_forest_chain_head {
    curl --silent -X POST -H "Content-Type: application/json" \
         --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.NetPeers","params":"null"}' \
         "http://127.0.0.1:${FOREST_RPC_PORT}/rpc/v0"
 }
 
 until call_forest_chain_head; do
     echo "Forest is unavailable - sleeping for 1s"
     sleep 1
 done

popd
