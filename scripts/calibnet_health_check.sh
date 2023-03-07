#!/usr/bin/env bash

set -e

SNAPSHOT_DIRECTORY="/tmp/snapshots"
LOG_DIRECTORY="/tmp/log"

FOREST_PATH="forest"
FOREST_CLI_PATH="forest-cli"

echo "Fetching params"
$FOREST_CLI_PATH fetch-params --keys
echo "Downloading snapshot"
$FOREST_CLI_PATH --chain calibnet snapshot fetch --aria2 -s $SNAPSHOT_DIRECTORY

echo "Importing snapshot and running Forest"
$FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --import-snapshot $SNAPSHOT_DIRECTORY/*.car
echo "Checking DB stats"
$FOREST_CLI_PATH --chain calibnet db stats
echo "Running forest in detached mode"
$FOREST_PATH --chain calibnet --encrypt-keystore false --log-dir $LOG_DIRECTORY --detach --save-token ./admin_token

echo "Validating checkpoint tipset hashes"
$FOREST_CLI_PATH chain validate-tipset-checkpoints

echo "Waiting for sync and check health"
timeout 30m $FOREST_CLI_PATH --chain calibnet sync wait && $FOREST_CLI_PATH --chain calibnet db stats
echo "Exporting snapshot"
$FOREST_CLI_PATH --chain calibnet snapshot export

echo "Verifing snapshot checksum"
sha256sum -c ./*.sha256sum

echo "Testing js console"
$FOREST_CLI_PATH attach --exec 'showPeers()'

echo "Validating as mainnet snapshot"
set +e
$FOREST_CLI_PATH --chain mainnet snapshot validate $SNAPSHOT_DIRECTORY/*.car --force && \
{ echo "mainnet snapshot validation with calibnet snapshot should fail"; return 1; }
set -e

echo "Validating as calibnet snapshot"
$FOREST_CLI_PATH --chain calibnet snapshot validate $SNAPSHOT_DIRECTORY/*.car --force

# echo "--- Forest STDOUT ---"; cat forest.out
# echo "--- Forest STDERR ---"; cat forest.err
# echo "--- Forest Prometheus metrics ---"; cat metrics.log

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
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet import scripts/preloaded_wallet.key
sleep 5s

echo "Fetching metrics"
wget -O metrics.log http://localhost:6116/metrics

sleep 5s

# Show balances
echo "Listing wallet balances"
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet list

echo "Creating a new address to send FIL to:"
ADDR_TWO=$($FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet new)
echo "$ADDR_TWO"
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet set-default "t1ac6ndwj6nghqbmtbovvnwcqo577p6ox2pt52q2y"

echo "Listing wallet balances"
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet list

echo "Sending FIL to the above address"
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" send "$ADDR_TWO" "$FIL_AMT"

echo "Checking balance of $ADDR_TWO..."

sleep 4m

# wallet list should contain address two with transfered FIL amount
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet list

ADDR_TWO_BALANCE=$($FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet balance "$ADDR_TWO")
if [ "$ADDR_TWO_BALANCE" != "$FIL_AMT" ]; then
  echo "token amount should match"
  exit 1
fi


echo "Exporting wallet with "
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet export "$ADDR_TWO" > addr_two_pkey.test.key
echo "Importing wallet"
# TODO: wipe wallet and import back with preloaded key
$FOREST_CLI_PATH --chain calibnet --token "$ADMIN_TOKEN" wallet import addr_two_pkey.test.key || true

echo "Get and print metrics and logs and stop forest"
wget -O metrics.log http://localhost:6116/metrics

$FOREST_CLI_PATH --token "$ADMIN_TOKEN" shutdown --force

# echo "--- Forest STDOUT ---"; cat forest.out
# echo "--- Forest STDERR ---"; cat forest.err
# echo "--- Forest Prometheus metrics ---"; cat metrics.log
