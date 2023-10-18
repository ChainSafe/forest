#!/bin/bash
#
# Compute code coverage for the following steps:
#
# - unit tests
# - snapshot downloading
# - validating 200 tipsets from the snapshot
# - syncing to HEAD
# - exporting a snapshot
# - send command
#
# llvm-cov can be installed by running: cargo install cargo-llvm-cov
#

set -euxo pipefail

TMP_DIR=$(mktemp --directory)
TOKEN_PATH="$TMP_DIR/forest_admin_token"

function cleanup {
  # echo Removing temporary directory $TMP_DIR
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

function cov {
    # echo Running: cargo llvm-cov --no-report run --bin="$1" -- "${@:2}"
    cargo llvm-cov --no-report run --bin="$1" -- "${@:2}"
}

cargo llvm-cov --workspace clean
cargo llvm-cov --workspace --no-report
cov forest-cli --chain calibnet db clean --force
cov forest-tool snapshot fetch --chain calibnet --vendor filops -s "$TMP_DIR"
SNAPSHOT_PATH=$(find "$TMP_DIR" -name \*.zst | head -n 1)
cov forest --chain calibnet --encrypt-keystore false --import-snapshot "$SNAPSHOT_PATH" --halt-after-import --height=-200 --track-peak-rss
cov forest-cli --chain calibnet db clean --force
cov forest-tool snapshot fetch --chain calibnet -s "$TMP_DIR"
SNAPSHOT_PATH=$(find "$TMP_DIR" -name \*.car | head -n 1)
cov forest --chain calibnet --encrypt-keystore false --import-snapshot "$SNAPSHOT_PATH" --height=-200 --detach --track-peak-rss --save-token "$TOKEN_PATH"
cov forest-cli sync wait
cov forest-cli sync status
cov forest-cli --chain calibnet db gc
cov forest-tool db stats --chain calibnet
cov forest-cli snapshot export
cov forest-cli snapshot export
cov forest-cli attach --exec 'showPeers()'
cov forest-cli net listen
cov forest-cli net peers
cov forest-cli mpool pending
cov forest-cli mpool stats
cov forest-cli net info

# Load the admin token
TOKEN=$(cat "$TOKEN_PATH")

# Get default address
DEFAULT_ADDR=$(cov forest-wallet --token "$TOKEN" default)

# Check that the address exists
cov forest-wallet --token "$TOKEN" has "$DEFAULT_ADDR" | grep "$DEFAULT_ADDR"

# Check that the address is listed
cov forest-wallet --token "$TOKEN" list | grep "$DEFAULT_ADDR"

# Generate new address
NEW_ADDR=$(cov forest-wallet --token "$TOKEN" new)

# Update default address
cov forest-wallet --token "$TOKEN" set-default "$NEW_ADDR"
cov forest-wallet --token "$TOKEN" default | grep "$NEW_ADDR"

# Sign a message
SIGNATURE=$(cov forest-wallet --token "$TOKEN" sign -a "$NEW_ADDR" -m deadbeef)
cov forest-wallet --token "$TOKEN" verify -a "$NEW_ADDR" -m deadbeef -s "$SIGNATURE" | grep true

# Check balance
cov forest-wallet --token "$TOKEN" balance "$NEW_ADDR" | grep 0

# Send funds
cov forest-cli --token "$TOKEN" send --from "$DEFAULT_ADDR" "$NEW_ADDR" 10attoFIL

# Create a read-only token
READ_TOKEN=$(cov forest-cli --token "$TOKEN" auth create-token --perm read)
# Make sure that viewing the wallet fails with the read-only token
cov forest-wallet --token "$READ_TOKEN" list && { echo "must fail"; return 1; }
# Verifying a message should still work with the read-only token
cov forest-wallet --token "$READ_TOKEN" verify -a "$NEW_ADDR" -m deadbeef -s "$SIGNATURE" | grep true

# Kill forest and generate coverage report
timeout 15 killall --wait --signal SIGINT forest
cargo llvm-cov report --lcov --output-path lcov.info

echo "Coverage data collected. You can view the report by running: genhtml lcov.info --output-directory=html"
