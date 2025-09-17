#!/usr/bin/env bash
# This script is checking the correctness of 
# the diff snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

format="${1:-v1}"

source "$(dirname "$0")/harness.sh"

forest_init "$@"

db_path=$($FOREST_TOOL_PATH db stats --chain calibnet | grep "Database path:" | cut -d':' -f2- | xargs)
snapshot=$(find "$db_path/car_db"/*.car.zst | tail -n 1)
snapshot_epoch=$(forest_query_epoch "$snapshot")

echo "Exporting diff snapshot @ $snapshot_epoch with forest-cli snapshot export-diff"
$FOREST_CLI_PATH snapshot export-diff --from "$snapshot_epoch" --to "$((snapshot_epoch - 900))" -d 900 -o diff1

$FOREST_CLI_PATH shutdown --force

echo "Exporting diff snapshot @ $snapshot_epoch with forest-tool archive export"
$FOREST_TOOL_PATH archive export --epoch "$snapshot_epoch" --depth 900 --diff "$((snapshot_epoch - 900))" --diff-depth 900 -o diff2 "$snapshot"

echo "Comparing diff snapshots"
diff diff1 diff2
