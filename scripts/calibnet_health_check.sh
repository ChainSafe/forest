#!/bin/bash

SNAPSHOT_DIRECTORY="/tmp/snapshots"
LOG_DIRECTORY="/tmp/log"

echo "Fetching params"
forest-cli fetch-params --keys
echo "Downloading snapshot"
forest-cli --chain calibnet snapshot fetch --aria2 -s $SNAPSHOT_DIRECTORY

echo "Importing snapshot and running Forest"
forest --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --import-snapshot $SNAPSHOT_DIRECTORY/*.car
echo "Checking DB stats"
forest-cli --chain calibnet db stats
echo "Running forest in detached mode"
forest --chain calibnet --encrypt-keystore false --log-dir $LOG_DIRECTORY --detach --save-token ./admin_token

echo "Validating checkpoint tipset hashes"
forest-cli chain validate-tipset-checkpoints

echo "Waiting for sync and check health"
timeout 30m forest-cli --chain calibnet sync wait && forest-cli --chain calibnet db stats
echo "Exporting snapshot"
forest-cli --chain calibnet snapshot export

echo "Verifing snapshot checksum"
sha256sum -c ./*.sha256sum

echo "Testing js console"
forest-cli attach --exec 'showPeers()'

echo "Validating as mainnet snapshot"
forest-cli --chain mainnet snapshot validate $SNAPSHOT_DIRECTORY/*.car --force && \
{ echo "mainnet snapshot validation with calibnet snapshot should fail"; return 1; }
echo "Validating as calibnet snapshot"
forest-cli --chain calibnet snapshot validate $SNAPSHOT_DIRECTORY/*.car --force

echo "--- Forest STDOUT ---"; cat forest.out
echo "--- Forest STDERR ---"; cat forest.err
echo "--- Forest Prometheus metrics ---"; cat metrics.log

echo "Print forest log files"
ls -hl $LOG_DIRECTORY
cat $LOG_DIRECTORY/*

echo "Wallet tests"

# The following steps does basic wallet handling tests.

# Amount to send to
FIL_AMT=500
# Admin token used when interacting with wallet
ADMIN_TOKEN=$(cat admin_token)
# Wallet addresses: 
# A preloaded address
ADDR_ONE=f1qmmbzfb3m6fijab4boagmkx72ouxhh7f2ylgzlq

echo "Importing preloaded wallet key"
forest-cli --chain calibnet --token "$ADMIN_TOKEN" wallet import scripts/preloaded_wallet.key
sleep 5s

echo "Fetching metrics"
wget -O metrics.log http://localhost:6116/metrics

sleep 5s

# Show balances
echo "Listing wallet balances"
forest-cli --chain calibnet --token "$ADMIN_TOKEN" wallet list

echo "Creating a new address to send FIL to"
ADDR_TWO=$(forest-cli --chain calibnet --token "$ADMIN_TOKEN" wallet new)

echo "Send FIL to the above address"
forest-cli --token "$ADMIN_TOKEN" send "$ADDR_TWO" "$FIL_AMT"

echo "Check balance of $ADDR_TWO"

sleep 3m

forest-cli --chain calibnet --token "$ADMIN_TOKEN" wallet balance "$ADDR_TWO"

echo "Exporting wallet with "
forest-cli --chain calibnet --token "$ADMIN_TOKEN" wallet export "$ADDR_ONE" > addr_two_pkey.test.key
echo "Importing wallet"
forest-cli --chain calibnet --token "$ADMIN_TOKEN" wallet import addr_two_pkey.test.key || true

# wallet list should contain address two will transfered FIL amount
forest-cli --chain calibnet --token "$ADMIN_TOKEN" wallet list

Get and print metrics and logs and kill forest
wget -O metrics.log http://localhost:6116/metrics
pkill forest
echo "--- Forest STDOUT ---"; cat forest.out
echo "--- Forest STDERR ---"; cat forest.err
echo "--- Forest Prometheus metrics ---"; cat metrics.log
