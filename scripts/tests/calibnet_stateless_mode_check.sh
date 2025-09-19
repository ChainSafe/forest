#!/bin/bash
set -euxo pipefail

# This script tests the stateless mode of a forest node

source "$(dirname "$0")/harness.sh"

forest_init_stateless

# Example format: /ip4/127.0.0.1/tcp/41937/p2p/12D3KooWAB9z7vZ1x1v9t4BViVkX1Hy1ScoRnWV2GgGy5ec6pfUZ
STATELESS_NODE_ADDRESS=$($FOREST_CLI_PATH net listen | tail -n 1)
echo "Stateless node address: $STATELESS_NODE_ADDRESS"
# Example format: 12D3KooWAB9z7vZ1x1v9t4BViVkX1Hy1ScoRnWV2GgGy5ec6pfUZ
STATELESS_NODE_PEER_ID=$(echo "$STATELESS_NODE_ADDRESS" | cut --delimiter="/" --fields=7 --zero-terminated)
echo "Stateless node peer id: $STATELESS_NODE_PEER_ID"

# Run a normal forest node that only connects to the stateless node
CONFIG_PATH="./forest_config.toml"
cat <<- EOF > $CONFIG_PATH
	[network]
	listening_multiaddrs = ["/ip4/127.0.0.1/tcp/0"]
	bootstrap_peers = ["$STATELESS_NODE_ADDRESS"]
	mdns = false
	kademlia = false
EOF

# Disable discovery to not connect to more nodes
$FOREST_PATH --chain calibnet --encrypt-keystore false --auto-download-snapshot --config "$CONFIG_PATH" --rpc false --metrics-address 127.0.0.1:6117 --healthcheck-address 127.0.0.1:2347 &
FOREST_NODE_PID=$!
# Verify that the stateless node can respond to chain exchange requests
until curl http://127.0.0.1:6117/metrics | grep "chain_exchange_response_in"; do
    sleep 1s;
done
kill -KILL $FOREST_NODE_PID
