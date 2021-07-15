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

AUTH_HEADER="Authorization: Bearer ${TOKEN}"
CONTENT_TYPE_HEADER="Content-Type: application/json-rpc"

RPC_ENDPOINTS=("WalletList" "WalletBalance")

for endpoint in ${RPC_ENDPOINTS[@]}; do
    METHOD="Filecoin.${endpoint}"
    REQUEST_BODY="{\"jsonrpc\": \"2.0\", \"method\": \"$METHOD\", \"params\":[], \"id\": 0}"

    RESPONSE_CODE=$(curl -w "%{http_code}" -s -o /dev/null -X POST -H "$CONTENT_TYPE_HEADER" -H "$AUTH_HEADER" -d "$REQUEST_BODY" http://127.0.0.1:1235/rpc/v0)

    echo $RESPONSE_CODE
done
