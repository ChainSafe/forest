#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

source "$(dirname "$0")/harness.sh"

forest_init

echo "Cleaning up the initial snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

echo "Exporting zstd compressed snapshot"
$FOREST_CLI_PATH snapshot export

echo "Testing snapshot validity"
zstd --test ./*.car.zst

echo "Verifying snapshot checksum"
sha256sum --check ./*.sha256sum

echo "Running snapshot diff test"
pushd "$(mktemp --directory)"
    imported_snapshot=$("$FOREST_CLI_PATH" --chain calibnet snapshot fetch --vendor forest | tail -1)
    zstd -d "$imported_snapshot"
    imported_snapshot=$(find . -type f -name "*.car" | head -1)
    imported_snapshot_without_ext=$(basename ./forest_snapshot_calibnet_2023-07-26_height_768225.car .car)
    height=$(echo "$imported_snapshot_without_ext" | grep -Eo '[0-9]+$')
    : : generating diffed snapshot
    "$FOREST_CLI_PATH" --chain calibnet archive export -e "$height" -d 1500 --diff 1000 imported_snapshot
rm -- *
popd

echo "Validating CAR files"
zstd --decompress ./*.car.zst
for f in *.car; do
  echo "Validating CAR file $f"
  $FOREST_CLI_PATH --chain calibnet snapshot validate "$f"
done
