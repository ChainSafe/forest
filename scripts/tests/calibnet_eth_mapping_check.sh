#!/usr/bin/env bash
# This script is checking the correctness of the ethereum mapping feature
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.



set -eu

source "$(dirname "$0")/harness.sh"

forest_init

NUM_TIPSETS=100

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

ERROR=0
echo "Testing Ethereum mapping"

for hash in "${ETH_BLOCK_HASHES[@]}"; do
  JSON=$(curl -s -X POST 'http://localhost:2345/rpc/v1' -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Filecoin.EthGetBalance\",\"params\":[\"0xff38c072f286e3b20b3954ca9f99c05fbecc64aa\", \"$hash\"]}")
  # echo "$JSON"
  if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
    echo "Missing tipset key for hash $hash"
    ERROR=1
  fi
done

for hash in "${ETH_TX_HASHES[@]}"; do
  JSON=$(curl -s -X POST 'http://localhost:2345/rpc/v1' -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Filecoin.EthGetMessageCidByTransactionHash\",\"params\":[\"$hash\"]}")
  # echo "$JSON"
  if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
    echo "Missing cid for hash $hash"
    ERROR=1
  fi
done

echo "Done"
if [[ $ERROR -ne 0 ]]; then
  exit 1
fi

# We now shutdown forest and restart it with ttl for the Ethereum mapping enabled

$FOREST_CLI_PATH shutdown --force

sleep 5

forest_run_node_mapping_ttl_detached

# Filecoin has a block time of around 30 seconds. Given a TTL of 600s for the mapping,
# if we retrieve Ethereum blocks of the last 20 tipsets and collect Ethereum tx hashes
# We should only be able to retrieve CIDs for the ones where age < TTL

NUM_TIPSETS=20

echo "Get Ethereum block hashes and transactions hashes from the last $NUM_TIPSETS tipsets"

OUTPUT=$($FOREST_CLI_PATH info show)

HEAD_EPOCH=$(echo "$OUTPUT" | sed -n 's/.*epoch: \([0-9]*\).*/\1/p')
EPOCH=$((HEAD_EPOCH - 1))

# Initialize arrays
ETH_TX_HASHES=()
TX_AGES=()

for ((i=0; i<=NUM_TIPSETS; i++)); do
  EPOCH_HEX=$(printf "0x%x" $EPOCH)
  #echo "$EPOCH_HEX"
  JSON=$(curl -s -X POST 'http://127.0.0.1:2345/rpc/v1' -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Filecoin.EthGetBlockByNumber\",\"params\":[\"$EPOCH_HEX\", false]}")
  #echo "$JSON"

  HASH=$(echo "$JSON" | jq -r '.result.hash')

  if [[ $(echo "$JSON" | jq -e '.result.transactions') != "null" ]]; then
    TRANSACTIONS=$(echo "$JSON" | jq -r '.result.transactions[]')
    TIMESTAMP=$(echo "$JSON" | jq -r '.result.timestamp')

    decimal_timestamp=$((TIMESTAMP))
    CURRENT_DATE_SEC=$(date +%s)
    TX_AGE_IN_SECS=$((CURRENT_DATE_SEC - decimal_timestamp))
    for tx in $TRANSACTIONS; do
        ETH_TX_HASHES+=("$tx")
        TX_AGES+=("$TX_AGE_IN_SECS")
        #echo "$tx"
    done
  else
    echo "No transactions found for block hash: $EPOCH_HEX"
  fi

  EPOCH=$((EPOCH - 1))
done

echo "Testing Ethereum transactions ttl"

for idx in "${!ETH_TX_HASHES[@]}"; do
  hash=${ETH_TX_HASHES[$idx]}
  TX_AGE=${TX_AGES[$idx]}
  JSON=$(curl -s -X POST 'http://localhost:2345/rpc/v1' -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Filecoin.EthGetMessageCidByTransactionHash\",\"params\":[\"$hash\"]}")
  # echo "$JSON"

  # We add 120 seconds of slack because GC only runs every 30 seconds
  # and is not instantaneous
  if (( TX_AGE > 720 )); then
    if [[ $(echo "$JSON" | jq -e '.result') != "null" ]]; then
      #echo "$JSON"
      echo "Found cid for hash $hash, mapping should be GCed (tx age: $TX_AGE seconds)"
      ERROR=1
    else
      echo "Ok"
    fi
  fi
  if (( TX_AGE <= 600 )); then
    if [[ $(echo "$JSON" | jq -e '.result') == "null" ]]; then
      echo "Missing cid for hash $hash (tx age: $TX_AGE seconds)"
      ERROR=1
    else
      echo "Ok"
    fi
  fi
done

echo "Done"

exit $ERROR
