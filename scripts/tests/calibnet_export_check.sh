#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -e

source "$(dirname "$0")/harness.sh"

forest_init

echo "Exporting zstd compressed snapshot"
$FOREST_CLI_PATH snapshot export

echo "Verifing snapshot checksum"
sha256sum -c ./*.sha256sum
