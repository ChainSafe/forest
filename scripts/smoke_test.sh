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

AUTH_HEADERS="{ \"Content-Type\": \"application/json-rpc\", \"Authorization\": \"Bearer ${TOKEN}\" }"

RPC_ENDPOINTS=("WalletList", "WalletBalance")

for endpoint in ${RPC_ENDPOINTS[@]}; do
    METHOD="Filecoin.${endpoint}"
    REQUEST_BODY="{\"jsonrpc\": \"2.0\", \"method\":$METHOD, \"params\":[], \"id\": 0}"

    OUTPUT=$(curl -s -X POST -H "$AUTH_HEADERS" -d "$REQUEST_BODY" http://127.0.0.1:1235/rpc/v0)

    echo $OUTPUT
    echo $?
done
