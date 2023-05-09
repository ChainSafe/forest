#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.
# It requires the database to be initialized and synced.

set -e

# Check if the database is initialized
if [ ! -d ~/.local/share/forest/calibnet ]; then
  echo "Database not initialized. Exiting."
  exit 1
fi

FOREST_PATH="forest"
FOREST_CLI_PATH="forest-cli"

TMP_DIR=$(mktemp --directory)
LOG_DIRECTORY=$TMP_DIR/logs

function cleanup {
  $FOREST_CLI_PATH shutdown --force

  timeout 10s sh -c "while pkill -0 forest 2>/dev/null; do sleep 1; done"
}
trap cleanup EXIT

echo "Running forest in detached mode"
$FOREST_PATH --chain calibnet --encrypt-keystore false --log-dir "$LOG_DIRECTORY" --detach --save-token ./admin_token --track-peak-rss

echo "Waiting for sync and check health"
timeout 30m $FOREST_CLI_PATH sync wait && $FOREST_CLI_PATH db stats

echo "Exporting uncompressed snapshot"
$FOREST_CLI_PATH snapshot export

echo "Verifing snapshot checksum"
sha256sum -c ./*.sha256sum

echo "Exporting zstd compressed snapshot"
$FOREST_CLI_PATH snapshot export --compressed

echo "Get and print metrics and logs and stop forest"
wget -O metrics.log http://localhost:6116/metrics

echo "--- Forest STDOUT ---"; cat forest.out
echo "--- Forest STDERR ---"; cat forest.err
echo "--- Forest Prometheus metrics ---"; cat metrics.log
