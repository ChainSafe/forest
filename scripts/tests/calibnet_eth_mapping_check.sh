#!/usr/bin/env bash
# This script is checking the correctness of the ethereum mapping feature
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

source "$(dirname "$0")/harness.sh"

forest_init

NUM_TIPSETS=200

echo "Get Ethereum block hashes and transactions hashes from the last $NUM_TIPSETS tipsets"

OUTPUT=$($FOREST_CLI_PATH info show)

HEAD_EPOCH=$(echo "$OUTPUT" | sed -n 's/.*epoch: \([0-9]*\).*/\1/p')
EPOCH=$((HEAD_EPOCH - 1))

# Initialize arrays and sets
ETH_BLOCK_HASHES=()
ETH_TX_HASHES=()

for ((i=0; i<=NUM_TIPSETS; i++)); do
  EPOCH_HEX=$(printf "0x%x" $EPOCH)
  #echo "$EPOCH_HEX"
  JSON=$(curl -s -X POST 'http://127.0.0.1:2345/rpc/v1' -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Filecoin.EthGetBlockByNumber\",\"params\":[\"$EPOCH_HEX\", false]}")
  #echo "$JSON"

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


# echo "ETH_BLOCK_HASHES: ${ETH_BLOCK_HASHES[@]}"
# echo "ETH_TX_HASHES: ${ETH_TX_HASHES[@]}"

echo "Use hashes to call Filecoin.EthGetBlockByHash and Filecoin.EthGetMessageCidByTransactionHash"

for hash in "${ETH_BLOCK_HASHES[@]}"; do
  JSON=$(curl -s -X POST 'http://localhost:2345/rpc/v1' -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Filecoin.EthGetBlockByHash\",\"params\":[\"$hash\", false]}")
  #echo "$JSON"
done

ERROR=0
for hash in "${ETH_TX_HASHES[@]}"; do
  JSON=$(curl -s -X POST 'http://localhost:2345/rpc/v1' -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Filecoin.EthGetMessageCidByTransactionHash\",\"params\":[\"$hash\"]}")
  if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
    echo "Missing result for hash $hash"
    ERROR=1
  fi
done

exit $ERROR
