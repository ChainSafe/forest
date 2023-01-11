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

## Wallet tests

FIL_AMT=500
ADMIN_TOKEN=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXSwiZXhwIjoxNjc4NTMwODk3fQ.E0Glkq-Br5x3wLCes8HS20wAfzlSGqVX-mo0WKeYXRc
# Wallet addresses: 
ADDR_ONE=f1qmmbzfb3m6fijab4boagmkx72ouxhh7f2ylgzlq
# ADDR_TWO=f1zy4ylxlqplz3ulqscmta4ay7aijphopmmmjcjqq

# Show balance
forest-cli --chain calibnet --token $ADMIN_TOKEN wallet list

# # create new address
# ADDR_TWO=`forest-cli --chain calibnet --token $ADMIN_TOKEN wallet new`

# # send FIL to the above address
# forest-cli --token send $ADDR_TWO $FIL_AMT

# # Check balance of addr_two
# timeout 30 min && ./target/release/forest-cli --chain calibnet --token $ADMIN_TOKEN wallet balance $ADDR_TWO
# # Export wallet
# forest-cli --chain calibnet --token $ADMIN_TOKEN wallet export $ADDR_ONE

# Get and print metrics and logs and kill forest
# wget -O metrics.log http://localhost:6116/metrics
# pkill forest
# echo "--- Forest STDOUT ---"; cat forest.out
# echo "--- Forest STDERR ---"; cat forest.err
# echo "--- Forest Prometheus metrics ---"; cat metrics.log
