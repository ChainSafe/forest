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

echo "Validating CAR files"
zstd --decompress ./*.car.zst
for f in *.car; do
  echo "Validating CAR file $f"
  $FOREST_CLI_PATH --chain calibnet snapshot validate "$f"
done
