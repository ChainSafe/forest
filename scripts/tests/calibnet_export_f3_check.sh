#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

source "$(dirname "$0")/harness.sh"

forest_init_with_f3

echo "Cleaning up the initial snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

echo "Wait for F3 to sync"
timeout 10m $FOREST_CLI_PATH f3 ready --wait --no-progress-timeout 5m

echo "Exporting zstd compressed snapshot in v2 format"
$FOREST_CLI_PATH snapshot export --format v2

$FOREST_CLI_PATH shutdown --force

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
