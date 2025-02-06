#!/bin/bash
set -euxo pipefail

# This script tests Forest is able to catch up the butterflynet.

source "$(dirname "$0")/harness.sh"

function call_forest_chain_head {
  curl --silent -X POST -H "Content-Type: application/json" \
        --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
        "http://127.0.0.1:2345/rpc/v1"
}

$FOREST_PATH --chain butterflynet --encrypt-keystore false &
FOREST_NODE_PID=$!

until call_forest_chain_head; do
    echo "Forest RPC endpoint is unavailable - sleeping for 1s"
    sleep 1
done

forest_wait_for_sync

function shutdown {
  kill -KILL $FOREST_NODE_PID
}

trap shutdown EXIT
