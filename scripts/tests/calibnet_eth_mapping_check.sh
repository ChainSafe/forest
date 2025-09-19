#!/usr/bin/env bash
# This script is checking the correctness of the ethereum mapping feature
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

source "$(dirname "$0")/harness.sh"

forest_init --backfill-db 200

FOREST_URL='http://127.0.0.1:2345/rpc/v1'

NUM_TIPSETS=200

echo "Get Ethereum block hashes and transactions hashes from the last $NUM_TIPSETS tipsets"

OUTPUT=$($FOREST_`CLI`_PATH info show)

HEAD_EPOCH=$(echo "$OUTPUT" | sed -n 's/.*epoch: \([0-9]*\).*/\1/p')
EPOCH=$((HEAD_EPOCH - 1))

ETH_BLOCK_HASHES=()
ETH_TX_HASHES=()

for ((i=0; i<=NUM_TIPSETS; i++)); do
  EPOCH_HEX=$(printf "0x%x" $EPOCH)
  JSON=$(curl -s -X POST "$FOREST_URL" \
    -H 'Content-Type: application/json' \
    --data "$(jq -n --arg epoch "$EPOCH_HEX" '{jsonrpc: "2.0", id: 1, method: "Filecoin.EthGetBlockByNumber", params: [$epoch, false]}')")


  HASH=$(echo "$JSON" | jq -r '.result.hash')
  ETH_BLOCK_HASHES+=("$HASH")

  if [[ $(echo "$JSON" | jq -e '.result.transactions') != "null" ]]; then
    TRANSACTIONS=$(echo "$JSON" | jq -r '.result.transactions[]')
    for tx in $TRANSACTIONS; do
        ETH_TX_HASHES+=("$tx")
    done
  else
    echo "No transactions found for block hash: $EPOCH_HEX"
  fi

  EPOCH=$((EPOCH - 1))
done

echo "Done"

forest_wait_for_healthcheck_ready

ERROR=0
echo "Testing Ethereum mappings"

for hash in "${ETH_BLOCK_HASHES[@]}"; do
  JSON=$(curl -s -X POST "$FOREST_URL" \
    -H 'Content-Type: application/json' \
    --data "$(jq -n --arg hash "$hash" '{jsonrpc: "2.0", id: 1, method: "Filecoin.EthGetBalance", params: ["0xff38c072f286e3b20b3954ca9f99c05fbecc64aa", $hash]}')")

  if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
    echo "Missing tipset key for hash $hash"
    ERROR=1
  fi
done

for hash in "${ETH_TX_HASHES[@]}"; do
  JSON=$(curl -s -X POST "$FOREST_URL" \
    -H 'Content-Type: application/json' \
    --data "$(jq -n --arg hash "$hash" '{jsonrpc: "2.0", id: 1, method: "Filecoin.EthGetMessageCidByTransactionHash", params: [$hash]}')")

  if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
    echo "Missing cid for hash $hash"
    ERROR=1
  fi
done

echo "Done"
exit $ERROR
