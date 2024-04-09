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

# Bootstrap a normal forest node with the stateless node
CONFIG_PATH="./forest_config.toml"
cat <<- EOF > $CONFIG_PATH
	[network]
	listening_multiaddrs = ["/ip4/127.0.0.1/tcp/0"]
	bootstrap_peers = ["$STATELESS_NODE_ADDRESS"]
	mdns = false
	kademlia = true
EOF

$FOREST_PATH --chain calibnet --encrypt-keystore false --auto-download-snapshot --config "$CONFIG_PATH" --save-token ./admin_token --rpc-address 127.0.0.1:12345 --metrics-address 127.0.0.1:6117 --healthcheck-address 127.0.0.1:2347 &
FOREST_NODE_PID=$!
# Verify that more peers are connected via kademlia
until (( $(curl http://127.0.0.1:6117/metrics | grep full_peers | tail -n 1 | cut --delimiter=" " --fields=2) > 1 )); do
    sleep 1s;
done

FULLNODE_API_INFO="$(cat admin_token):/ip4/127.0.0.1/tcp/12345/http" $FOREST_CLI_PATH sync wait # allow the node to re-sync

kill -KILL $FOREST_NODE_PID
