#!/usr/bin/env bash
# This script is to check the correctness of the Forest ETH filter API

set -eu

source "$(dirname "$0")/harness.sh"

forest_init

FOREST_URL='http://127.0.0.1:2345/rpc/v1'

# Create a new filter and capture the returned filter ID
FILTER=$(curl -s -X POST "$FOREST_URL" -H "Content-Type: application/json" --data '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "Filecoin.EthNewFilter",
    "params": [{
      "address": [],
      "topics": null,
      "blockHash": null
    }]
}')

# Extract the filter ID from the response
FILTER_ID=$(echo "$FILTER" | jq -r '.result')

# Check if FILTER_ID is valid
if [[ -z "$FILTER_ID" || "$FILTER_ID" == "null" ]]; then
    echo "Failed to retrieve filter ID."
    exit 1
fi

echo "Filter created with ID: $FILTER_ID"

# Use the filter ID to get logs
FILTER_LOGS=$(curl -s -X POST "$FOREST_URL" -H "Content-Type: application/json" -H "Authorization: Bearer $ADMIN_TOKEN" --data '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "Filecoin.EthGetFilterLogs",
    "params": ["'"$FILTER_ID"'"]
}')

echo "Filter Logs: $FILTER_LOGS"

# Uninstall the filter
UNINSTALL=$(curl -s -X POST "$FOREST_URL" -H "Content-Type: application/json" --data '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "Filecoin.EthUninstallFilter",
    "params": ["'"$FILTER_ID"'"]
}')

echo "Uninstall Filter: $UNINSTALL"
