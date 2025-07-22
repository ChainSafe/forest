#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

source "$(dirname "$0")/harness.sh"

forest_init "$@"

echo "Cleaning up the initial snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

echo "Exporting zstd compressed snapshot"
$FOREST_CLI_PATH snapshot export -o v1.forest.car.zst

echo "Exporting zstd compressed snapshot in the experimental v2 format"
$FOREST_CLI_PATH snapshot export --format v2 -o v2.forest.car.zst

echo "Inspecting archive info and metadata"
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

echo "Validating CAR files"
for f in *.car.zst; do
  echo "Validating CAR file $f"
  $FOREST_TOOL_PATH snapshot validate "$f"
done

echo "Exporting zstd compressed snapshot at genesis"
$FOREST_CLI_PATH snapshot export --tipset 0

echo "Testing genesis snapshot validity"
zstd --test forest_snapshot_calibnet_2022-11-01_height_0.forest.car.zst
