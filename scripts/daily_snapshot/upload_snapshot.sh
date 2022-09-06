#!/bin/bash

set -e

dnf install -y docker docker-compose

DETACH_TIMEOUT=20s
SYNC_TIMEOUT=25m

CHAIN_NAME=$1
NEWEST_SNAPSHOT=$2

cd "$BASE_FOLDER"

echo "Running forest daemon"
echo "Chain: $CHAIN_NAME"
echo "Snapshot: $NEWEST_SNAPSHOT"
#docker network create snapshot_forest || true
# docker run \
#   --name snapshot_forest \
#   --rm \
#   --detach \
#   --network host \
#   -v "$BASE_FOLDER":"$BASE_FOLDER":rshared \
#   ghcr.io/chainsafe/forest:${FOREST_TAG} \
#   --encrypt-keystore false --rpc-address 0.0.0.0:1234 --metrics-address 0.0.0.0:6116 --chain "$CHAIN_NAME" --import-snapshot "$NEWEST_SNAPSHOT"

echo "Waiting for daemon"
# Wait for the RPC endpoint to be available. Remove this once Forest support the --detach flag.
sleep "$DETACH_TIMEOUT"

docker logs forest-calibnet

echo "Waiting for sync"
# Wait for forest node to be completely synced.
timeout "$SYNC_TIMEOUT" docker run --env FULLNODE_API_INFO=/dns4/forest-calibnet/tcp/12345/http --network default_calibnet --rm ghcr.io/chainsafe/forest sync wait
echo "Synced to calibnet"

echo "No recent snapshot. Exporting new snapshot."
docker run --rm --network snapshot_forest ghcr.io/chainsafe/forest chain export
echo "Export done. Uploading.."
mv ./forest_snapshot* s3/calibnet/
echo "Upload done."
docker stop snapshot_forest
