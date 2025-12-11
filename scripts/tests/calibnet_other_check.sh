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

echo "Test subcommand: state actor-cids"
bundle_cid=$($FOREST_CLI_PATH state actor-cids --format json | jq -r '.Bundle["/"]')
manifest_path="./build/manifest.json"
if ! grep -q "$bundle_cid" "$manifest_path"; then
  echo "Bundle CID $bundle_cid not found in $manifest_path"
  exit 1
fi

echo "Regression testing mempool select"
gem install http --user-install
$FOREST_CLI_PATH chain head --format json -n 1000 | scripts/mpool_select_killer.rb

echo "Test subcommand: state compute (batch)"
head_epoch=$($FOREST_CLI_PATH chain head --format json | jq ".[0].epoch")
if ! [[ "$head_epoch" =~ ^[0-9]+$ ]]; then
  echo "Failed to parse numeric head epoch from 'chain head --format json': $head_epoch"
  exit 1
fi
start_epoch=$(( head_epoch > 900 ? head_epoch - 900 : 0 ))
$FOREST_CLI_PATH state compute --epoch "$start_epoch" -n 10 -v

echo "Validating metrics"
wget -O metrics.log http://localhost:6116/metrics
go run ./tools/prometheus_metrics_validator metrics.log

echo "Testing RPC batch request"
batch_response=$(curl -s -X POST http://localhost:2345/rpc/v1 -H "Content-Type: application/json" --data '[{"jsonrpc":"2.0","method":"Filecoin.Version","id":1},{"jsonrpc":"2.0","method":"Filecoin.Session","id":2}]')
block_delay=$(echo "$batch_response" | jq -r '.[0].result.BlockDelay')
if [ "$block_delay" != "30" ]; then
  echo "Invalid block delay: $block_delay"
  exit 1
fi
session=$(echo "$batch_response" | jq -r '.[1].result')
if [ "$session" == "" ]; then
  echo "Invalid session: $session"
  exit 1
fi

# Assert invalid API path returns HTTP 404
echo "Testing invalid API path handling"
status_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST http://localhost:2345/rpc/v3 -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"Filecoin.Version","id":1}')
if [ "$status_code" != "404" ]; then
  echo "Unexpected status code for invalid RPC path: $status_code"
  exit 1
fi

# Assert unsupported method returns HTTP 200 but RPC error code -32601
echo "Testing unsupported RPC method handling"
unsupported_response=$(curl -s -X POST http://localhost:2345/rpc/v1 -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"Invoke.Cthulhu","params":[],"id":1}')
error_code=$(echo "$unsupported_response" | jq -r '.error.code')
if [ "$error_code" != "-32601" ]; then
  echo "Unexpected error code for unsupported method: $error_code"
  exit 1
fi
