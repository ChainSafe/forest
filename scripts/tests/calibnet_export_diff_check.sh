#!/usr/bin/env bash
# This script is checking the correctness of 
# the diff snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -euo pipefail

source "$(dirname "$0")/harness.sh"

forest_init "$@"

retries=30
sleep_interval=0.5

db_path=$($FOREST_TOOL_PATH db stats --chain calibnet | grep "Database path:" | cut -d':' -f2- | xargs)
snapshot=$(find "$db_path/car_db"/*.car.zst | tail -n 1)
snapshot_epoch=$(forest_query_epoch "$snapshot")

echo "Exporting diff snapshot @ $snapshot_epoch with forest-cli snapshot export-diff"
$FOREST_CLI_PATH snapshot export-diff --from "$snapshot_epoch" --to "$((snapshot_epoch - 900))" -d 900 -o diff1 &

echo "Testing that export is in progress"
for ((i=1; i<=retries; i++)); do
    output=$($FOREST_CLI_PATH snapshot export-status --format json)
    state=$(echo "$output" | jq -r '.state')
    if [ "$state" == "Running" ]; then
        break
    fi
    if [ $i -eq $retries ]; then
        echo "export should be in progress"
        exit 1
    fi
    sleep $sleep_interval
done

$FOREST_CLI_PATH snapshot export-cancel

echo "Testing that export has been cancelled"
for ((i=1; i<=retries; i++)); do
    output=$($FOREST_CLI_PATH snapshot export-status --format json)
    state=$(echo "$output" | jq -r '.state')
    if [ "$state" == "Cancelled" ]; then
        break
    fi
    if [ $i -eq $retries ]; then
        echo "export should be cancelled"
        exit 1
    fi
    sleep $sleep_interval
done

echo "Exporting diff snapshot @ $snapshot_epoch with forest-cli snapshot export-diff"
$FOREST_CLI_PATH snapshot export-diff --from "$snapshot_epoch" --to "$((snapshot_epoch - 900))" -d 900 -o diff1 &
EXPORT_CMD_PID=$!
sleep 5
# another export job should be disallowed
export_error=$($FOREST_CLI_PATH snapshot export 2>&1 || true)
if echo "$export_error" | grep -q "export has been running since"; then
    echo "verified another export job is disallowed"
else 
    echo "another export job should be disallowed"
    echo "output was: $export_error"
    exit 1
fi
# another export-diff job should be disallowed
export_diff_error=$($FOREST_CLI_PATH snapshot export-diff --from 11000 --to 10100 -d 900 2>&1 || true)
if echo "$export_diff_error" | grep -q "export has been running since"; then
    echo "verified another export-diff job is disallowed"
else 
    echo "another export-diff job should be disallowed"
    echo "output was: $export_diff_error"
    exit 1
fi
# Killing the CLI should not cancel the export
kill -KILL $EXPORT_CMD_PID
# Wait on the same export job
$FOREST_CLI_PATH snapshot export-status --wait

$FOREST_CLI_PATH shutdown --force

echo "Exporting diff snapshot @ $snapshot_epoch with forest-tool archive export"
$FOREST_TOOL_PATH archive export --epoch "$snapshot_epoch" --depth 900 --diff "$((snapshot_epoch - 900))" --diff-depth 900 -o diff2 "$snapshot"

echo "Comparing diff snapshots"
diff diff1 diff2
