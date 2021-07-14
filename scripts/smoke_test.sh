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

TOKEN="$(cut -d':' -f1 <<< $ADDR)"

AUTH_HEADERS="Authorization: Bearer ${TOKEN}"

curl --write-out "WalletList %{http_code}\n" -s -X POST -H 'Content-Type: application/json-rpc' $AUTH_HEADERS -d '{"jsonrpc": "2.0", "method":"Filecoin.WalletList", "params":[], "id": 0}' http://127.0.0.1:1235/rpc/v0

echo $?
