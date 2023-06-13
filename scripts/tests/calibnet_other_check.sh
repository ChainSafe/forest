#!/usr/bin/env bash
# This script checks various features of the forest node
# and the forest-cli.
# It requires both `forest` and `forest-cli` to be in the PATH.

set -e

source "$(dirname "$0")/harness.sh"

forest_init

echo "Validating checkpoint tipset hashes"
$FOREST_CLI_PATH chain validate-tipset-checkpoints

echo "Running database garbage collection"
forest_check_db_stats
$FOREST_CLI_PATH db gc
forest_check_db_stats

echo "Testing js console"
$FOREST_CLI_PATH attach --exec 'showPeers()'

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

echo "Test subcommand: info show"
$FOREST_CLI_PATH info show

$FOREST_CLI_PATH sync wait # allow the node to re-sync
