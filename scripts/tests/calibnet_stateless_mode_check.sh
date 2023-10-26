#!/bin/bash
set -euxo pipefail

# This script tests the stateless mode of a forest node

source "$(dirname "$0")/harness.sh"

forest_init_stateless

echo "Verifying the heaviest tipset to be the genesis"
HEAD_CID=$($FOREST_CLI_PATH chain head | jq -r '.[0]')
assert_eq "$HEAD_CID" "bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4"

STATELESS_NODE_ADDRESS=$($FOREST_CLI_PATH net listen | tail -n 1)
echo "Stateless node address: $STATELESS_NODE_ADDRESS"
STATELESS_NODE_PEER_ID=$(echo "$STATELESS_NODE_ADDRESS" | cut --delimiter="/" --fields=7 --zero-terminated)
echo "Stateless node peer id: $STATELESS_NODE_PEER_ID"

# Run a normal forest node that only connects to the stateless node
CONFIG_PATH="./forest_config.toml"
cat <<- EOF > $CONFIG_PATH
	[network]
	listening_multiaddrs = ["/ip4/127.0.0.1/tcp/0"]
	bootstrap_peers = ["$STATELESS_NODE_ADDRESS"]
EOF

$FOREST_PATH --chain calibnet --encrypt-keystore false --auto-download-snapshot --config "$CONFIG_PATH" --rpc false --metrics-address 127.0.0.1:6117 &
FOREST_NODE_PID=$!
# Verify that the stateless node can respond to chain exchange requests
until curl http://127.0.0.1:6117/metrics | grep "peer_chain_exchange_success{PEER=\"${STATELESS_NODE_PEER_ID}\"}"; do
    sleep 1s;
done
kill -KILL $FOREST_NODE_PID
