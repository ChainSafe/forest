#!/bin/bash
# Assert Forest and Lotus both climb past TARGET_HEIGHT (Curio is mining through Forest).

set -eux

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
pushd "${PARENT_PATH}"
# shellcheck source=/dev/null
source .env

function get_sync_height {
  local port=$1
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
       "http://127.0.0.1:${port}/rpc/v1" | jq '.result.Height'
}

start_time=$(date +%s)
timeout=$((start_time + 900))  # 15 minutes for Yugabyte + Curio cold start
target_height=$TARGET_HEIGHT

while true; do
  forest_height=$(get_sync_height "${FOREST_RPC_PORT}")
  lotus_height=$(get_sync_height "${LOTUS_RPC_PORT}")
  if [ "$forest_height" -gt "$target_height" ] && [ "$lotus_height" -gt "$target_height" ]; then
    echo "Both nodes past $target_height (Forest: $forest_height, Lotus: $lotus_height): Lotus validated Forest/Curio-produced blocks"
    break
  fi

  current_time=$(date +%s)
  if [ "$current_time" -gt "$timeout" ]; then
    echo "Timeout reached (Forest: $forest_height, Lotus: $lotus_height), target $target_height not reached by both"
    exit 1
  fi

  sleep 1
done

popd
