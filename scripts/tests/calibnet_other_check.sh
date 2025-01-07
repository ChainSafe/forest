#!/usr/bin/env bash
# This script checks various features of the forest node
# and the forest-cli.
# It requires both `forest` and `forest-cli` to be in the PATH.

set -e

source "$(dirname "$0")/harness.sh"

forest_import_non_calibnet_snapshot
forest_init

echo "Running Go F3 RPC client tests"
go test -v ./f3-sidecar

echo "Verifying the non calibnet snapshot (./test-snapshots/chain4.car) is being served properly."
$FOREST_CLI_PATH chain read-obj -c bafy2bzacedjrqan2fwfvhfopi64yickki7miiksecglpeiavf7xueytnzevlu

echo "Test subcommand: state compute"
cid=$($FOREST_CLI_PATH state compute --epoch 0)
# Expected state root CID, same reported as in Lotus. This should break only if the network is reset.
if [ "$cid" != "bafy2bzacecgqgzh3gxpariy3mzqb37y2vvxoaw5nwbrlzkhso6owus3zqckwe" ]; then
  echo "Unexpected state root CID: $cid"
  exit 1
fi

echo "Test dev commands (which could brick the node/cause subsequent snapshots to fail)"

echo "Test subcommand: chain set-head"
$FOREST_CLI_PATH chain set-head --epoch -10 --force

echo "Test subcommand: chain head"
$FOREST_CLI_PATH chain head
$FOREST_CLI_PATH chain head --tipsets 10

echo "Test subcommand: info show"
$FOREST_CLI_PATH info show

echo "Test subcommand: net info"
$FOREST_CLI_PATH net info

$FOREST_CLI_PATH sync wait # allow the node to re-sync

# Verify F3 is getting certificates from the network
until [[ $($FOREST_CLI_PATH f3 certs get --output json | jq '.GPBFTInstance') -gt 100 ]]; do
    sleep 1s;
done
