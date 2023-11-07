#!/usr/bin/env bash
# This script checks various features of the forest node
# and the forest-cli.
# It requires both `forest` and `forest-cli` to be in the PATH.

set -e

source "$(dirname "$0")/harness.sh"

forest_import_non_calibnet_snapshot
forest_init

echo "Verifying the non calibnet snapshot (./test-snapshots/chain4.car) is being served properly."
$FOREST_CLI_PATH chain read-obj -c bafy2bzacedjrqan2fwfvhfopi64yickki7miiksecglpeiavf7xueytnzevlu

echo "Running database garbage collection"
forest_check_db_stats
$FOREST_CLI_PATH db gc
forest_check_db_stats

echo "Testing js console"
$FOREST_CLI_PATH attach --exec 'showPeers()'

echo "Test dev commands (which could brick the node/cause subsequent snapshots to fail)"

echo "Test subcommand: chain set-head"
$FOREST_CLI_PATH chain set-head --epoch -10 --force

echo "Test subcommand: info show"
$FOREST_CLI_PATH info show

echo "Test subcommand: net info"
$FOREST_CLI_PATH net info

$FOREST_CLI_PATH sync wait # allow the node to re-sync
