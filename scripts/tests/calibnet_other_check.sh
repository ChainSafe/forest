#!/usr/bin/env bash
# This script checks various features of the forest node
# and the forest-cli.
# It requires both `forest` and `forest-cli` to be in the PATH.

set -e

source "$(dirname "$0")/harness.sh"

forest_import_non_calibnet_snapshot
forest_init "$@"

echo "Running Go F3 RPC client tests"
go test -v ./f3-sidecar

echo "Verifying the non calibnet snapshot (./test-snapshots/chain4.car) is being served properly."
$FOREST_CLI_PATH chain read-obj -c bafy2bzacedjrqan2fwfvhfopi64yickki7miiksecglpeiavf7xueytnzevlu

echo "Test subcommand: state compute at epoch 0"
cid=$($FOREST_CLI_PATH state compute --epoch 0)
# Expected state root CID, same reported as in Lotus. This should break only if the network is reset.
if [ "$cid" != "bafy2bzacecgqgzh3gxpariy3mzqb37y2vvxoaw5nwbrlzkhso6owus3zqckwe" ]; then
  echo "Unexpected state root CID: $cid"
  exit 1
fi

forest_check_db_stats
echo "Run snapshot GC"
$FOREST_CLI_PATH chain prune snap
forest_wait_api
echo "Wait the node to sync"
forest_wait_for_sync
forest_check_db_stats

echo "Test dev commands (which could brick the node/cause subsequent snapshots to fail)"

echo "Test subcommand: chain set-head"
$FOREST_CLI_PATH chain set-head --epoch -10 --force

echo "Test subcommand: chain head"
$FOREST_CLI_PATH chain head
$FOREST_CLI_PATH chain head --tipsets 10
$FOREST_CLI_PATH chain head --tipsets 5 --format json | jq 'length == 5'

echo "Test subcommand: info show"
$FOREST_CLI_PATH info show

echo "Test subcommand: net info"
$FOREST_CLI_PATH net info

$FOREST_CLI_PATH sync wait # allow the node to re-sync

echo "Test subcommand: healthcheck live"
$FOREST_CLI_PATH healthcheck live --wait

echo "Test subcommand: healthcheck healthy"
$FOREST_CLI_PATH healthcheck healthy --wait

echo "Test subcommand: healthcheck ready"
$FOREST_CLI_PATH healthcheck ready --wait

echo "Regression testing mempool select"
gem install http --user-install
$FOREST_CLI_PATH chain head --format json -n 1000 | scripts/mpool_select_killer.rb

echo "Test subcommand: state compute (batch)"
head=$($FOREST_CLI_PATH chain head | head -n 1 | jq ".[0]")
$FOREST_CLI_PATH state compute --epoch $((head - 900)) -n 10 -v
