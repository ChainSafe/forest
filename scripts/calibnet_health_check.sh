#!/bin/bash

SNAPSHOT_DIRECTORY="/tmp/snapshots"

# Fetch params
forest-cli fetch-params --keys
# Download snapshot
forest-cli --chain calibnet snapshot fetch --aria2 -s $SNAPSHOT_DIRECTORY

# Import snapshot and run Forest
forest --chain calibnet --target-peer-count 50 --encrypt-keystore false --halt-after-import --height=-200 --import-snapshot $SNAPSHOT_DIRECTORY/*.car
forest-cli --chain calibnet db stats
forest --chain calibnet --target-peer-count 50 --encrypt-keystore false --detach

# Validate checkpoint tipset hashes
forest-cli chain validate-tipset-checkpoints

# wait for sync and check health
timeout 30m forest-cli sync wait && forest-cli --chain calibnet db stats
# Export snapshot
forest-cli snapshot export

# verify snapshot checksum
sha256sum -c *.sha256sum

# js console
forest-cli attach --exec 'showPeers()'

# validate snapshot
forest-cli --chain mainnet snapshot validate $SNAPSHOT_DIRECTORY/*.car --force &&
{ echo "mainnet snapshot validation with calibnet snapshot should fail"; return 1; }
forest-cli --chain calibnet snapshot validate $SNAPSHOT_DIRECTORY/*.car --force

# Print forest logs
wget -O metrics.log http://localhost:6116/metrics
pkill forest
echo "--- Forest STDOUT ---"; cat forest.out
echo "--- Forest STDERR ---"; cat forest.err
echo "--- Forest Prometheus metrics ---"; cat metrics.log

# print forest log files
ls -hl log
cat log/*