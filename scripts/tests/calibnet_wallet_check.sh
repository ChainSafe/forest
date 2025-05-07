#!/usr/bin/env bash
# This script checks wallet features of the forest node and the forest-cli.
# It also checks some RPC methods that need a remote wallet.
# It requires both `forest` and `forest-cli` to be in the PATH.

set -euxo pipefail

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

forest_init "$@"

$FOREST_WALLET_PATH import preloaded_wallet.key
$FOREST_WALLET_PATH --remote-wallet import preloaded_wallet.key

# Test commented out due to it being flaky. See the tracking issue: https://github.com/ChainSafe/forest/issues/4849
# : Begin Filecoin.MarketAddBalance test
# 
# FOREST_URL='http://127.0.0.1:2345/rpc/v1'
# 
# # Amount to add to the Market actor (in attoFIL)
# MARKET_FIL_AMT="23"
# 
# # The preloaded address
# REMOTE_ADDR=$($FOREST_WALLET_PATH --remote-wallet list | tail -1 | cut -d ' ' -f1)
# 
# JSON=$(curl -s -X POST "$FOREST_URL" \
#   --header 'Accept: application/json' \
#   --header 'Content-Type: application/json' \
#   --header "Authorization: Bearer $ADMIN_TOKEN" \
#   --data "$(jq -n --arg addr "$REMOTE_ADDR" --arg amt "$MARKET_FIL_AMT" '{jsonrpc: "2.0", id: 1, method: "Filecoin.MarketAddBalance", params: [$addr, $addr, $amt]}')")
# 
# echo "$JSON"
# 
# if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
#   echo "Error while sending message."
#   exit 1
# fi
# 
# MSG_CID=$(echo "$JSON" | jq -r '.result["/"]')
# echo "Message cid: $MSG_CID"
# 
# # Try 30 times (in other words wait for 5 tipsets)
# for i in {1..30}
# do
#   sleep 5s
#   echo "Attempt $i:"
#   
#   JSON=$(curl -s -X POST "$FOREST_URL" \
#     --header 'Content-Type: application/json' \
#     --data "$(jq -n --arg cid "$MSG_CID" '{jsonrpc: "2.0", id: 1, method: "Filecoin.StateSearchMsg", params: [[], {"/": $cid}, 800, true]}')")
# 
#   echo "$JSON"
#   
#   # Check if the message has been mined.
#   if echo "$JSON" | jq -e '.result' > /dev/null; then
#     echo "Message found, exiting."
#     break
#   fi
# 
#   echo -e "\n"
# done
# 
# if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
#   echo "Error while sending message."
#   exit 1
# fi

: Begin wallet tests

# The following steps do basic wallet handling tests.

# Amount to send to 2nd address (note: `send` command defaults to FIL if no units are specified)
FIL_AMT="500 atto FIL"

# Amount for an empty wallet
FIL_ZERO="0 FIL"

# The preloaded address
ADDR_ONE=$($FOREST_WALLET_PATH list | tail -1 | cut -d ' ' -f1)

sleep 5s

$FOREST_WALLET_PATH export "$ADDR_ONE" > preloaded_wallet.test.key
$FOREST_WALLET_PATH delete "$ADDR_ONE"
$FOREST_WALLET_PATH --remote-wallet delete "$ADDR_ONE"
ROUNDTRIP_ADDR=$($FOREST_WALLET_PATH import preloaded_wallet.test.key)
if [[ "$ADDR_ONE" != "$ROUNDTRIP_ADDR" ]]; then
    echo "Wallet address should be the same after a roundtrip"
    exit 1
fi

ROUNDTRIP_ADDR=$($FOREST_WALLET_PATH --remote-wallet import preloaded_wallet.test.key)
if [[ "$ADDR_ONE" != "$ROUNDTRIP_ADDR" ]]; then
    echo "Wallet address should be the same after a roundtrip"
    exit 1
fi

wget -O metrics.log http://localhost:6116/metrics

sleep 5s

# Show balances
$FOREST_WALLET_PATH list

echo "Creating a new address to send FIL to"
ADDR_TWO=$($FOREST_WALLET_PATH new)
echo "$ADDR_TWO"
$FOREST_WALLET_PATH set-default "$ADDR_ONE"

echo "Creating a new (remote) address to send FIL to"
ADDR_THREE=$($FOREST_WALLET_PATH --remote-wallet new)
echo "$ADDR_THREE"
$FOREST_WALLET_PATH --remote-wallet set-default "$ADDR_ONE"

$FOREST_WALLET_PATH list
$FOREST_WALLET_PATH --remote-wallet list

MSG=$($FOREST_WALLET_PATH send "$ADDR_TWO" "$FIL_AMT")
: "$MSG"

MSG_REMOTE=$($FOREST_WALLET_PATH --remote-wallet send "$ADDR_THREE" "$FIL_AMT")
: "$MSG_REMOTE"

ADDR_TWO_BALANCE=$FIL_ZERO
i=0
while [[ $i != 20 && $ADDR_TWO_BALANCE == "$FIL_ZERO" ]]; do
  i=$((i+1))
  
  : "Checking balance $i/20"
  sleep 30s
  ADDR_TWO_BALANCE=$($FOREST_WALLET_PATH balance "$ADDR_TWO" --exact-balance)
done

ADDR_THREE_BALANCE=$FIL_ZERO
i=0
while [[ $i != 20 && $ADDR_THREE_BALANCE == "$FIL_ZERO" ]]; do
  i=$((i+1))

  : "Checking balance $i/20"
  sleep 30s
  ADDR_THREE_BALANCE=$($FOREST_WALLET_PATH --remote-wallet balance "$ADDR_THREE" --exact-balance)
done

# wallet list should contain address two with transfered FIL amount
$FOREST_WALLET_PATH list
$FOREST_WALLET_PATH --remote-wallet list

# wallet delete tests
ADDR_DEL=$(forest-wallet new)

forest-wallet delete "$ADDR_DEL"

# Validate that the wallet no longer exists.
forest-wallet list | grep --null-data --invert-match "${ADDR_DEL}"

# wallet delete tests
ADDR_DEL=$(forest-wallet --remote-wallet new)

forest-wallet --remote-wallet delete "$ADDR_DEL"

# Validate that the wallet no longer exists.
forest-wallet --remote-wallet list | grep --null-data --invert-match "${ADDR_DEL}"

# TODO: Uncomment this check once the send command is fixed
# # `$ADDR_TWO_BALANCE` is unitless (`list` command formats "500" as "500 atto FIL"),
# # so we need to truncate units from `$FIL_AMT` for proper comparison
# FIL_AMT=$(echo "$FIL_AMT"| cut -d ' ' -f 1)
# if [ "$ADDR_TWO_BALANCE" != "$FIL_AMT" ]; then
#   echo "FIL amount should match"
#   exit 1
# fi
