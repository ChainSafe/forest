#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

format="${1:-v1}"

source "$(dirname "$0")/harness.sh"

forest_init "$@"

echo "Cleaning up the initial snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

echo "Exporting zstd compressed snapshot at genesis"
$FOREST_CLI_PATH snapshot export --tipset 0 --format "$format"

echo "Exporting zstd compressed snapshot in $format format"
$FOREST_CLI_PATH snapshot export --format "$format"

snapshot=$(find ./*.car.zst | tail -n 1)
snapshot_epoch=$(forest_query_epoch "$snapshot")

echo "Exporting diff snapshot @ $snapshot_epoch with forest-cli snapshot export-diff"
$FOREST_CLI_PATH snapshot export-diff --from "$snapshot_epoch" --to "$((snapshot_epoch - 900))" -d 900 -o diff1

$FOREST_CLI_PATH shutdown --force

echo "Exporting diff snapshot @ $snapshot_epoch with forest-tool archive export"
$FOREST_TOOL_PATH archive export --epoch "$snapshot_epoch" --depth 900 --diff "$((snapshot_epoch - 900))" --diff-depth 900 -o diff2 "$snapshot"

echo "Comparing diff snapshots"
diff diff1 diff2

for f in *.car.zst; do
  echo "Inspecting archive info $f"
  $FOREST_TOOL_PATH archive info "$f"
  echo "Inspecting archive metadata $f"
  $FOREST_TOOL_PATH archive metadata "$f"
done

echo "Testing snapshot validity"
zstd --test ./*.car.zst

echo "Verifying snapshot checksum"
sha256sum --check ./*.sha256sum

for f in *.car.zst; do
  echo "Validating CAR file $f"
  $FOREST_TOOL_PATH snapshot validate "$f"
done
