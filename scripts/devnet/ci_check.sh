#!/bin/bash
# This script is used to check the correctness
# of the local devnet in the CI.

set -eux

# Path to the directory containing this script.
PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
pushd "${PARENT_PATH}"
source .env

# Forest check - assert that we sync past the genesis block.
# Allow for 300 seconds of sync time.
function get_sync_height {
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
       "http://127.0.0.1:${FOREST_RPC_PORT}/rpc/v0" | jq '.result.Height'
}

start_time=$(date +%s)
timeout=$((start_time + 300))  # Set timeout to 10 minutes

# Target height set so that all migrations are applied.
target_height=10

while true; do
  height=$(get_sync_height)
  if [ "$height" -gt "$target_height" ]; then
    echo "Height is larger than $target_height: $height"
    break
  fi

  current_time=$(date +%s)
  if [ "$current_time" -gt "$timeout" ]; then
    echo "Timeout reached, height is still not larger than $target_height: $height"
    exit 1
  fi

  sleep 1
done
