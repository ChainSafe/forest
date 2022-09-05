#!/bin/bash

set -e

DETACH_TIMEOUT=10m
SYNC_TIMEOUT=25m

CHAIN_NAME=$1
NEWEST_SNAPSHOT=$2

cd "$BASE_FOLDER"

forest --encrypt-keystore false --metrics-address 0.0.0.0:6116 --chain "$CHAIN_NAME" --import-snapshot "$NEWEST_SNAPSHOT" &
FOREST_PID=$!

# Wait for the RPC endpoint to be available. Remove this once Forest support the --detach flag.
sleep "$DETACH_TIMEOUT"

# Wait for forest node to be completely synced.
timeout "$SYNC_TIMEOUT" forest sync wait
echo "Synced to calibnet"

echo "No recent snapshot. Exporting new snapshot."
forest chain export
echo "Export done. Uploading.."
mv ./forest_snapshot* s3/calibnet/
echo "Upload done."
kill "$FOREST_PID"
