#!/usr/bin/env bash
# This script is used to test the `forest-cli` commands that do not
# require a running `forest` node.
# It depends on the `forest-cli` binary being in the PATH.

set -e

source "$(dirname "$0")/harness.sh"

SNAPSHOT_DIRECTORY=$TMP_DIR/snapshots

echo "Fetching params"
$FOREST_CLI_PATH fetch-params --keys
echo "Downloading zstd compressed snapshot without aria2"
$FOREST_CLI_PATH --chain calibnet snapshot fetch --provider filecoin --compressed -s "$SNAPSHOT_DIRECTORY"
echo "Downloading snapshot without aria2"
$FOREST_CLI_PATH --chain calibnet snapshot fetch --provider filecoin -s "$SNAPSHOT_DIRECTORY"
echo "Cleaning up snapshots"
$FOREST_CLI_PATH --chain calibnet snapshot clean -s "$SNAPSHOT_DIRECTORY" --force
echo "Cleaning up snapshots again"
$FOREST_CLI_PATH --chain calibnet snapshot clean -s "$SNAPSHOT_DIRECTORY" --force
echo "Downloading zstd compressed snapshot"
$FOREST_CLI_PATH --chain calibnet snapshot fetch --aria2 --provider filecoin --compressed -s "$SNAPSHOT_DIRECTORY"
echo "Cleaning up database"
$FOREST_CLI_PATH --chain calibnet db clean --force
echo "Cleaning up database again"
$FOREST_CLI_PATH --chain calibnet db clean --force

echo "Downloading snapshot"
$FOREST_CLI_PATH --chain calibnet snapshot fetch --aria2 -s "$SNAPSHOT_DIRECTORY"

echo "Validating as mainnet snapshot"
set +e
$FOREST_CLI_PATH --chain mainnet snapshot validate "$SNAPSHOT_DIRECTORY"/*.car --force && \
{
    echo "mainnet snapshot validation with calibnet snapshot should fail";
    exit 1;
}
set -e

echo "Validating as calibnet snapshot (uncompressed)"
$FOREST_CLI_PATH --chain calibnet snapshot validate "$SNAPSHOT_DIRECTORY"/*.car --force

echo "Validating as calibnet snapshot (compressed)"
$FOREST_CLI_PATH --chain calibnet snapshot validate "$SNAPSHOT_DIRECTORY"/*.zst --force
