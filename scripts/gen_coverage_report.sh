#!/bin/bash
#
# Compute code coverage for the following steps:
#
# - unit tests
# - snapshot downloading
# - validating 200 tipsets from the snapshot
# - syncing to HEAD
# - exporting a snapshot
#
# llvm-cov can be installed by running: cargo install cargo-llvm-cov
#

set +e

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
cargo llvm-cov --workspace --no-report --features slow_tests
cov forest-cli --chain calibnet db clean --force
cov forest-cli --chain calibnet snapshot fetch --aria2 --provider filecoin --compressed -s "$TMP_DIR"
SNAPSHOT_PATH=$(find "$TMP_DIR" -name \*.zst | head -n 1)
cov forest --chain calibnet --encrypt-keystore false --import-snapshot "$SNAPSHOT_PATH" --halt-after-import --height=-200 --track-peak-rss
cov forest-cli --chain calibnet db clean --force
cov forest-cli --chain calibnet snapshot fetch --aria2 -s "$TMP_DIR"
SNAPSHOT_PATH=$(find "$TMP_DIR" -name \*.car | head -n 1)
cov forest --chain calibnet --encrypt-keystore false --import-snapshot "$SNAPSHOT_PATH" --height=-200 --detach --track-peak-rss --save-token "$TOKEN_PATH"
cov forest-cli sync wait
cov forest-cli sync status
cov forest-cli chain validate-tipset-checkpoints
cov forest-cli --chain calibnet db gc
cov forest-cli --chain calibnet db stats
cov forest-cli snapshot export
cov forest-cli snapshot export --compressed
cov forest-cli attach --exec 'showPeers()'
cov forest-cli net listen
cov forest-cli net peers

# Load the admin token
TOKEN=$(cat "$TOKEN_PATH")

# Get default address
DEFAULT_ADDR=$(cov forest-cli --token "$TOKEN" wallet default)

# Check that the address exists
cov forest-cli --token "$TOKEN" wallet has "$DEFAULT_ADDR" | grep "$DEFAULT_ADDR"

# Check that the address is listed
cov forest-cli --token "$TOKEN" wallet list | grep "$DEFAULT_ADDR"

# Generate new address
NEW_ADDR=$(cov forest-cli --token "$TOKEN" wallet new)

# Update default address
cov forest-cli --token "$TOKEN" wallet set-default "$NEW_ADDR"
cov forest-cli --token "$TOKEN" wallet default | grep "$NEW_ADDR"

# Sign a message
SIGNATURE=$(cov forest-cli --token "$TOKEN" wallet sign -a "$NEW_ADDR" -m deadbeef)
cov forest-cli --token "$TOKEN" wallet verify -a "$NEW_ADDR" -m deadbeef -s "$SIGNATURE" | grep true

# Check balance
cov forest-cli --token "$TOKEN" wallet balance "$NEW_ADDR" | grep 0

# Create a read-only token
READ_TOKEN=$(cov forest-cli --token "$TOKEN" auth create-token --perm read)
# Make sure that viewing the wallet fails with the read-only token
cov forest-cli --token "$READ_TOKEN" wallet list && { echo "must fail"; return 1; }
# Verifying a message should still work with the read-only token
cov forest-cli --token "$READ_TOKEN" wallet verify -a "$NEW_ADDR" -m deadbeef -s "$SIGNATURE" | grep true

# Kill forest and generate coverage report
timeout 15 killall --wait --signal SIGINT forest
cargo llvm-cov report --lcov --output-path lcov.info

echo "Coverage data collected. You can view the report by running: genhtml lcov.info --output-directory=html"
