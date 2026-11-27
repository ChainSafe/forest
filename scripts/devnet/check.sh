#!/bin/bash
# This script is used to check the correctness
# of the local devnet in the CI.

set -eux

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
pushd "${PARENT_PATH}"
# shellcheck source=/dev/null
source .env
# shellcheck source=/dev/null
source "${PARENT_PATH}/chain_height.sh"

function get_f3_latest_cert_instance {
  local port=$1
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.F3GetLatestCertificate","param":"null"}' \
       "http://127.0.0.1:${port}/rpc/v1" | jq '.result.GPBFTInstance'
}

# Target height set so that all migrations are applied.
target_height=$TARGET_HEIGHT

# waits node to reach $target_height, timeout
docker exec \
  -e FULLNODE_API_INFO="/ip4/127.0.0.1/tcp/${FOREST_RPC_PORT}/http" \
  forest \
  timeout 6m \
  forest-cli sync wait --target "$target_height"

# Offline RPC should be at genesis height 0.
height=$(get_sync_height "${FOREST_OFFLINE_RPC_PORT}")
if [ "$height" != "0" ]; then
  echo "Offline RPC height is not zero: $height"
  exit 1
fi

# Check the F3 RPC
height=$(get_f3_latest_cert_instance ${FOREST_RPC_PORT})
if [ "$height" -lt 1 ]; then
  echo "latest cert instance should be greater than zero: $height"
  exit 1
fi
