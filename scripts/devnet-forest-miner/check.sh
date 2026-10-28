#!/bin/bash
# Assert that Lotus Miner drives Forest past the target height AND that the Lotus
# validating node follows it there. Both climbing proves block production through
# Forest's RPC works and that Lotus accepts (validates) Forest-produced blocks.

set -eux

# Path to the directory containing this script.
PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
pushd "${PARENT_PATH}"
source .env

function get_sync_height {
  local port=$1
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
       "http://127.0.0.1:${port}/rpc/v1" | jq '.result.Height'
}

start_time=$(date +%s)
timeout=$((start_time + 360))  # 6 minutes

# Target height chosen so that every migration in .env is crossed.
target_height=$TARGET_HEIGHT

while true; do
  forest_height=$(get_sync_height "${FOREST_RPC_PORT}")
  lotus_height=$(get_sync_height "${LOTUS_RPC_PORT}")
  if [ "$forest_height" -gt "$target_height" ] && [ "$lotus_height" -gt "$target_height" ]; then
    echo "Both nodes past $target_height (Forest: $forest_height, Lotus: $lotus_height): Lotus validated Forest-produced blocks"
    break
  fi

  current_time=$(date +%s)
  if [ "$current_time" -gt "$timeout" ]; then
    echo "Timeout reached (Forest: $forest_height, Lotus: $lotus_height), target $target_height not reached by both"
    exit 1
  fi

  sleep 1
done
