#!/usr/bin/env bash
# This script checks some RPC methods of the forest node.
# It requires both `forest` and `forest-wallet` to be in the PATH.

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

forest_init

: Begin Filecoin.MarketAddBalance test

FOREST_URL='http://127.0.0.1:2345/rpc/v1'

$FOREST_WALLET_PATH --remote-wallet import preloaded_wallet.key

# The preloaded address
ADDR_ONE=$($FOREST_WALLET_PATH list | tail -1 | cut -d ' ' -f1)

# Amount to add to the Market actor (in attoFIL)
FIL_AMT="23"

JSON=$(curl -s -X POST "$FOREST_URL" \
  --header 'Accept: application/json' \
  --header 'Content-Type: application/json' \
  --header "Authorization: Bearer $ADMIN_TOKEN" \
  --data "$(jq -n --arg addr "$ADDR_ONE" --arg amt "$FIL_AMT" '{jsonrpc: "2.0", id: 1, method: "Filecoin.MarketAddBalance", params: [$addr, $addr, $amt]}')")

echo "$JSON"

if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
  echo "Error while sending message."
  exit 1
fi

MSG_CID=$(echo "$JSON" | jq -r '.result["/"]')
echo "Message cid: $MSG_CID"

# Try 10 times.
for i in {1..10}
do
  sleep 30s
  echo "Attempt $i:"
  
  JSON=$(curl -s -X POST "$FOREST_URL" \
    --header 'Content-Type: application/json' \
    --data "$(jq -n --arg cid "$MSG_CID" '{jsonrpc: "2.0", id: 1, method: "Filecoin.StateSearchMsg", params: [{"/": $cid}]}')")

  echo "$JSON"
  
  # Check if the message has been mined.
  if echo "$JSON" | jq -e '.result' > /dev/null; then
    echo "Message found, exiting."
    exit 0
  fi

  echo -e "\n"
done

exit 1
