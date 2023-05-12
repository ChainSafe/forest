#!/usr/bin/env bash
# This script checks wallet features of the forest node and the forest-cli.
# It requires both `forest` and `forest-cli` to be in the PATH.

set -e

source "$(dirname "$0")/harness.sh"


usage() {
  echo "Usage: $0 <PRELOADED_WALLET_STRING>"
  exit 1
}

if [ -z "$1" ]
  then
    usage
fi

echo "$1" > preloaded_wallet.key

forest_init

echo "Wallet tests"

# The following steps do basic wallet handling tests.

# Amount to send to 2nd address (note: `send` command defaults to FIL if no units are specified)
FIL_AMT="500 atto FIL"

echo "Importing preloaded wallet key"
$FOREST_CLI_PATH wallet import preloaded_wallet.key

# The preloaded address
ADDR_ONE=$($FOREST_CLI_PATH wallet list | tail -1 | cut -d ' ' -f1)

sleep 5s

echo "Exporting key"
$FOREST_CLI_PATH wallet export "$ADDR_ONE" > preloaded_wallet.test.key
if ! cmp -s preloaded_wallet.key preloaded_wallet.test.key; then
    echo ".key files should match"
    exit 1
fi

echo "Fetching metrics"
wget -O metrics.log http://localhost:6116/metrics

sleep 5s

# Show balances
echo "Listing wallet balances"
$FOREST_CLI_PATH wallet list

echo "Creating a new address to send FIL to"
ADDR_TWO=$($FOREST_CLI_PATH wallet new)
echo "$ADDR_TWO"
$FOREST_CLI_PATH wallet set-default "$ADDR_ONE"

echo "Listing wallet balances"
$FOREST_CLI_PATH wallet list

echo "Sending FIL to the above address"
MSG=$($FOREST_CLI_PATH send "$ADDR_TWO" "$FIL_AMT")
echo "Message cid:"
echo "$MSG"

echo "Checking balance of $ADDR_TWO..."

ADDR_TWO_BALANCE=0
i=0
while [[ $i != 20 && $ADDR_TWO_BALANCE == 0 ]]; do
  i=$((i+1))
  
  echo "Checking balance $i/20"
  sleep 30s
  ADDR_TWO_BALANCE=$($FOREST_CLI_PATH wallet balance "$ADDR_TWO")
done

# wallet list should contain address two with transfered FIL amount
$FOREST_CLI_PATH wallet list

# TODO: Uncomment this check once the send command is fixed
# # `$ADDR_TWO_BALANCE` is unitless (`list` command formats "500" as "500 atto FIL"),
# # so we need to truncate units from `$FIL_AMT` for proper comparison
# FIL_AMT=$(echo "$FIL_AMT"| cut -d ' ' -f 1)
# if [ "$ADDR_TWO_BALANCE" != "$FIL_AMT" ]; then
#   echo "FIL amount should match"
#   exit 1
# fi
