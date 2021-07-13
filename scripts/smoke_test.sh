#!/bin/bash

if [ "$1" == "help" ]; then
    echo "Smoke Test\nRun forest node; set FULLNODE_API_INFO; smoke test CLI"
    exit
fi

ADDR=$FULLNODE_API_INFO

if [ -z "$ADDR" ]; then
    echo "FULLNODE_API_INFO is not set. Use $0 help for usage"
    exit
fi

SPLIT_ADDR=$(echo $ADDR | tr ":" "\n")
TOKEN=$SPLIT_ADDR[0]

echo "Smoke testing Wallet"

# command
# curl -s -X POST -H 'Content-Type: application/json-rpc' -H 'Authorization: Bearer <token>'
# -d '{"jsonrpc": "2.0", "method":"Filecoin.<method>", "params":[], "id": 0}' http://127.0.0.1:1234/rpc/v0

OUTPUT=$(curl -s -X POST -H 'Content-Type: application/json-rpc' -H 'Authorization: Bearer ${TOKEN}' -d '{"jsonrpc": "2.0", "method":"Filecoin.WalletDefault", "params":[], "id": 0}' http://127.0.0.1:1235/rpc/v0)

echo $OUTPUT
echo $?
