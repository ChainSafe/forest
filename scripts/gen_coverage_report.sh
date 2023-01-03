#!/bin/bash
#
# Compute code coverage for the following steps:
#
# - unit tests
# - snapshot dowloading
# - validating 200 tipsets from the snapshot
# - syncing to HEAD
# - exporting a snapshot
#
# llvm-cov can be installed by running: cargo install cargo-llvm-cov
#

set +e

cargo llvm-cov --workspace clean
cargo llvm-cov --workspace --no-report --features slow_tests
cargo llvm-cov --no-report run --bin=forest-cli -- --chain calibnet db clean --force
cargo llvm-cov --no-report run --bin=forest-cli -- --chain calibnet snapshot fetch -s .
# SC2012 says to use 'find' instead of 'ls' but I don't know how to do that.
# shellcheck disable=SC2012
cargo llvm-cov --no-report run --bin=forest -- --chain calibnet --encrypt-keystore false --import-snapshot "$(ls -1t ./*.car | head -n 1)" --height=-200 --detach
cargo llvm-cov --no-report run --bin=forest-cli -- sync wait
cargo llvm-cov --no-report run --bin=forest-cli -- sync status
cargo llvm-cov --no-report run --bin=forest-cli -- chain validate-tipset-checkpoints
cargo llvm-cov --no-report run --bin=forest-cli -- snapshot export

TOKEN=$(grep "Admin token" forest.out  | cut -d ' ' -f 7)

# Get default address
DEFAULT_ADDR=$(cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet default)

# Check that the address exists
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet has "$DEFAULT_ADDR" | grep "$DEFAULT_ADDR"

# Check that the address is listed
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet list | grep "$DEFAULT_ADDR"

# Generate new address
NEW_ADDR=$(cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet new)

# Update default address
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet set-default "$NEW_ADDR"
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet default | grep "$NEW_ADDR"

# Sign a message
SIGNATURE=$(cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet sign -a "$NEW_ADDR" -m deadbeef)
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet verify -a "$NEW_ADDR" -m deadbeef -s "$SIGNATURE" | grep true

# Check balance
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" wallet balance "$NEW_ADDR" | grep 0

# Create a read-only token
READ_TOKEN=$(cargo llvm-cov --no-report run --bin=forest-cli -- --token "$TOKEN" auth create-token --perm read)
# Make sure that viewing the wallet fails with the read-only token
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$READ_TOKEN" wallet list && { echo "must fail"; return 1; }
# Verifying a message should still work with the read-only token
cargo llvm-cov --no-report run --bin=forest-cli -- --token "$READ_TOKEN" wallet verify -a "$NEW_ADDR" -m deadbeef -s "$SIGNATURE" | grep true





# Kill forest and generate coverage report
timeout 15 killall --wait --signal SIGINT forest
cargo llvm-cov report --lcov --output-path lcov.info

echo "Coverage data collected. You can view the report by running: genhtml lcov.info --output-directory=html"

# basic: 67.2% 14795 of 22009 lines

# with sleep: 67.2% (14800 of 22009 lines)

# with validated checkpoints: 67.3% (14807 of 22009 lines)