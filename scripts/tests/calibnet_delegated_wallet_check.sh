#!/usr/bin/env bash
# This script checks delegated wallet features of the forest node and the forest-cli.
# It requires both `forest` and `forest-cli` to be in the PATH.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

forest_wallet_init "$@"

# Amount to send (note: `send` command defaults to FIL if no units are specified)
FIL_AMT="500 atto FIL"

# Amount for an empty wallet
FIL_ZERO="0 FIL"

# The preloaded address
ADDR_ONE=$($FOREST_WALLET_PATH list | tail -1 | cut -d ' ' -f2)

sleep 5s

: Begin delegated wallet tests

# The following steps do basic delegated wallet handling tests.

echo "Creating delegated wallet DELEGATE_ADDR_ONE"
DELEGATE_ADDR_ONE=$($FOREST_WALLET_PATH new delegated)
echo "$DELEGATE_ADDR_ONE"
$FOREST_WALLET_PATH export "$DELEGATE_ADDR_ONE" > delegated_wallet.key
$FOREST_WALLET_PATH --remote-wallet import delegated_wallet.key

# Fund delegated wallet from preloaded wallet
DELEGATE_FUND_AMT="3 micro FIL"
$FOREST_WALLET_PATH set-default "$ADDR_ONE"
MSG_DELEGATE_FUND=$($FOREST_WALLET_PATH send "$DELEGATE_ADDR_ONE" "$DELEGATE_FUND_AMT")
: "$MSG_DELEGATE_FUND"

DELEGATE_ADDR_ONE_BALANCE=$FIL_ZERO
i=0
while [[ $i != 20 && $DELEGATE_ADDR_ONE_BALANCE == "$FIL_ZERO" ]]; do
  i=$((i+1))
  : "Checking DELEGATE_ADDR_ONE balance $i/20"
  sleep 30s
  DELEGATE_ADDR_ONE_BALANCE=$($FOREST_WALLET_PATH balance "$DELEGATE_ADDR_ONE" --exact-balance)
done

echo "Creating delegated wallet DELEGATE_ADDR_TWO"
DELEGATE_ADDR_TWO=$($FOREST_WALLET_PATH new delegated)
echo "$DELEGATE_ADDR_TWO"
$FOREST_WALLET_PATH set-default "$DELEGATE_ADDR_ONE"

echo "Creating delegated (remote) wallet DELEGATE_ADDR_THREE"
DELEGATE_ADDR_THREE=$($FOREST_WALLET_PATH --remote-wallet new delegated)
echo "$DELEGATE_ADDR_THREE"
$FOREST_WALLET_PATH --remote-wallet set-default "$DELEGATE_ADDR_ONE"

$FOREST_WALLET_PATH list
$FOREST_WALLET_PATH --remote-wallet list

MSG_DELEGATE_TWO=$($FOREST_WALLET_PATH send "$DELEGATE_ADDR_TWO" "$FIL_AMT")
: "$MSG_DELEGATE_TWO"

MSG_DELEGATE_THREE=$($FOREST_WALLET_PATH send "$DELEGATE_ADDR_THREE" "$FIL_AMT")
: "$MSG_DELEGATE_THREE"

DELEGATE_ADDR_TWO_BALANCE=$FIL_ZERO
i=0
while [[ $i != 20 && $DELEGATE_ADDR_TWO_BALANCE == "$FIL_ZERO" ]]; do
  i=$((i+1))
  : "Checking DELEGATE_ADDR_TWO balance $i/20"
  sleep 30s
  DELEGATE_ADDR_TWO_BALANCE=$($FOREST_WALLET_PATH balance "$DELEGATE_ADDR_TWO" --exact-balance)
done

DELEGATE_ADDR_THREE_BALANCE=$FIL_ZERO
i=0
while [[ $i != 20 && $DELEGATE_ADDR_THREE_BALANCE == "$FIL_ZERO" ]]; do
  i=$((i+1))
  : "Checking DELEGATE_ADDR_THREE balance $i/20"
  sleep 30s
  DELEGATE_ADDR_THREE_BALANCE=$($FOREST_WALLET_PATH --remote-wallet balance "$DELEGATE_ADDR_THREE" --exact-balance)
done

$FOREST_WALLET_PATH list
$FOREST_WALLET_PATH --remote-wallet list

MSG_DELEGATE_FOUR=$($FOREST_WALLET_PATH --remote-wallet send "$DELEGATE_ADDR_THREE" "$FIL_AMT")
: "$MSG_DELEGATE_FOUR"

DELEGATE_ADDR_REMOTE_THREE_BALANCE=$DELEGATE_ADDR_THREE_BALANCE
i=0
while [[ $i != 20 && $DELEGATE_ADDR_REMOTE_THREE_BALANCE == "$DELEGATE_ADDR_THREE_BALANCE" ]]; do
  i=$((i+1))
  : "Checking DELEGATE_ADDR_THREE balance $i/20"
  sleep 30s
  DELEGATE_ADDR_REMOTE_THREE_BALANCE=$($FOREST_WALLET_PATH --remote-wallet balance "$DELEGATE_ADDR_THREE" --exact-balance)
done

$FOREST_WALLET_PATH list
$FOREST_WALLET_PATH --remote-wallet list

: End delegated wallet tests
