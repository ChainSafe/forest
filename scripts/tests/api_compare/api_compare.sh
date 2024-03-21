#!/bin/bash
# This script is used to compare the data between Forest and Lotus nodes using the comparison tool.
# The only requirement here is `docker`. `docker compose` must  be running, see `setup.sh` for more details.

set -uo pipefail

# Path to the directory containing this script.
PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" || exit; pwd -P)
pushd "${PARENT_PATH}" || exit
source .env

# Retry logic for syncing nodes
function wait_for_sync() {
    local retries=10
    local delay=30
    for ((i = 1; i <= retries; i++)); do
        echo "Attempt $i: Wait for Forest & Lotus to sync.."
        if docker compose exec forest forest-cli sync wait && docker compose exec lotus lotus sync wait; then
            echo "Both nodes synced successfully."
            return 0
        else
            echo "Sync attempt $i failed. Retrying in $delay seconds..."
            sleep "$delay"
        fi
    done
    echo "Failed to sync nodes after $retries attempts."
    return 1
}

if ! wait_for_sync; then
    echo "Failed to sync nodes. Exiting..." >&2
    exit 1
fi

# We need the network name to attach the comparison tool to the same network as the nodes.
COMPOSE_NETWORK=$(docker compose config --format json | jq '.networks."api-tests".name' | tr -d '"')
COMPOSE_VOLUME=$(docker compose config --format json | jq '.volumes."node-data".name' | tr -d '"')
FOREST_ADDRESS="/dns/forest/tcp/$FOREST_RPC_PORT/http"
LOTUS_ADDRESS="/dns/lotus/tcp/$LOTUS_RPC_PORT/http"
# get file name in /data/snapshot/ directory
SNAPSHOT_NAME=$(docker run --rm -v "$COMPOSE_VOLUME":/data --entrypoint ls "$FOREST_IMAGE" /data/snapshot | grep forest.car.zst)
if ! docker run --rm --network="$COMPOSE_NETWORK" \
   -v "$(pwd)":/data/tester/ \
   -v "$COMPOSE_VOLUME":/data \
   --entrypoint forest-tool "$FOREST_IMAGE" \
   api compare \
   /data/snapshot/"$SNAPSHOT_NAME" \
   --forest "$FOREST_ADDRESS" \
   --lotus "$LOTUS_ADDRESS" \
   --n-tipsets 5; then
    echo "Comparison tool failed to execute. Exiting..." >&2
    exit 1
fi
