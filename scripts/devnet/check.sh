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
  local port=$1
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
       "http://127.0.0.1:${port}/rpc/v1" | jq '.result.Height'
}

function get_f3_latest_cert_instance {
  local port=$1
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.`F3`GetLatestCertificate","param":"null"}' \
       "http://127.0.0.1:${port}/rpc/v1" | jq '.result.GPBFTInstance'
}

start_time=$(date +%s)
timeout=$((start_time + 300))  # Set timeout to 5 minutes

# Target height set so that all migrations are applied.
target_height=$TARGET_HEIGHT

while true; do
  height=$(get_sync_height ${FOREST_RPC_PORT})
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

# Check the offline RPC, which should be initialized at that point. It should be at the genesis height, so 0.
height=$(get_sync_height ${FOREST_OFFLINE_RPC_PORT})
if [ "$height" -ne 0 ]; then
  echo "Offline RPC height is not zero: $height"
  exit 1
fi

# Check the `F3` RPC
height=$(get_f3_latest_cert_instance ${FOREST_RPC_PORT})
if [ "$height" -lt 1 ]; then
  echo "latest cert instance should be greater than zero: $height"
  exit 1
fi
