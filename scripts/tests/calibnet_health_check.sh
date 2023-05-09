#!/usr/bin/env bash
# This script checks various features of the forest node
# and the forest-cli.
# It requires both `forest` and `forest-cli` to be in the PATH.
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

echo "Checking DB stats"
$FOREST_CLI_PATH --chain calibnet db stats

echo "Running forest in detached mode"
$FOREST_PATH --chain calibnet --encrypt-keystore false --log-dir "$LOG_DIRECTORY" --detach --save-token ./admin_token --track-peak-rss

echo "Validating checkpoint tipset hashes"
$FOREST_CLI_PATH chain validate-tipset-checkpoints

echo "Waiting for sync and check health"
timeout 30m $FOREST_CLI_PATH sync wait && $FOREST_CLI_PATH db stats

# Admin token used when interacting with wallet
ADMIN_TOKEN=$(cat admin_token)
# Set environment variable
export FULLNODE_API_INFO="$ADMIN_TOKEN:/ip4/127.0.0.1/tcp/1234/http"

echo "Running database garbage collection"
du -hS ~/.local/share/forest/calibnet
$FOREST_CLI_PATH db gc
du -hS ~/.local/share/forest/calibnet

echo "Testing js console"
$FOREST_CLI_PATH attach --exec 'showPeers()'

echo "Print forest log files"
ls -hl "$LOG_DIRECTORY"
cat "$LOG_DIRECTORY"/*

# Get the checkpoint hash at epoch 424000. This output isn't affected by the
# number of recent state roots we store (2k at the time of writing) and this
# output should never change.
echo "Checkpoint hash test"
EXPECTED_HASH="Chain:           calibnet
Epoch:           424000
Checkpoint hash: 8cab45fd441c1fb68d2fd7e45d5e9ef9a5d3b45f68b414ab3e244233dd8e37fc4dacffc8966b2dc8804d4abf92c8a57efda743e26db6805a77a4feac19478293"
ACTUAL_HASH=$($FOREST_CLI_PATH --chain calibnet chain tipset-hash 424000)
if [[ $EXPECTED_HASH != "$ACTUAL_HASH" ]]; then
  printf "Invalid tipset hash:\n%s" "$ACTUAL_HASH"
  printf "Expected:\n%s" "$EXPECTED_HASH"
  exit 1
fi

echo "Test dev commands (which could brick the node/cause subsequent snapshots to fail)"

echo "Test subcommand: chain set-head"
$FOREST_CLI_PATH chain set-head --epoch -10 --force

$FOREST_CLI_PATH sync wait # allow the node to re-sync

echo "Get and print metrics and logs and stop forest"
wget -O metrics.log http://localhost:6116/metrics

echo "--- Forest STDOUT ---"; cat forest.out
echo "--- Forest STDERR ---"; cat forest.err
echo "--- Forest Prometheus metrics ---"; cat metrics.log
