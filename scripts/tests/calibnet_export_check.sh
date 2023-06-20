#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -e

source "$(dirname "$0")/harness.sh"

forest_init

echo "Cleaning up the initial snapshot"
rm -rf ./*.car.*

echo "Exporting zstd compressed snapshot"
$FOREST_CLI_PATH snapshot export

echo "Testing snapshot validity"
zstd --test ./*.car.zst

echo "Verifying snapshot checksum"
sha256sum --check ./*.sha256sum
