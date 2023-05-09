#!/usr/bin/env bash
# This script initializes the Forest node.
# It requires `forest` and `forest-cli` binaries to be in the PATH.

set -e

FOREST_PATH="forest"
FOREST_CLI_PATH="forest-cli"

TMP_DIR=$(mktemp --directory)
SNAPSHOT_DIRECTORY=$TMP_DIR/snapshots

echo "Fetching calibnet snapshot"
$FOREST_CLI_PATH --chain calibnet snapshot fetch --aria2 -s "$SNAPSHOT_DIRECTORY"

echo "Importing snapshot and running Forest"
$FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --import-snapshot "$SNAPSHOT_DIRECTORY"/*.car

echo "Checking DB stats"
$FOREST_CLI_PATH --chain calibnet db stats
