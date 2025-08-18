#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

source "$(dirname "$0")/harness.sh"

forest_init "$@"

echo "Cleaning up the initial snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

echo "Exporting zstd compressed snapshot with unordred graph traversal"
$FOREST_CLI_PATH snapshot export --unordered -o unordered.forest.car.zst

$FOREST_CLI_PATH shutdown --force

for f in *.car.zst; do
  echo "Inspecting archive info $f"
  $FOREST_TOOL_PATH archive info "$f"
  echo "Inspecting archive metadata $f"
  $FOREST_TOOL_PATH archive metadata "$f"
done

echo "Cleanup calibnet db"
$FOREST_TOOL_PATH db destroy --chain calibnet --force

echo "Import the unordered snapshot"
$FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --height=-100 --import-snapshot unordered.forest.car.zst

echo "Check if Forest is able to sync"
forest_run_node_detached
forest_wait_api
forest_wait_for_sync
forest_wait_for_healthcheck_ready
