#!/bin/bash
set -euxo pipefail

# This script tests the stateless mode of a forest node

source "$(dirname "$0")/harness.sh"

forest_init_stateless

echo "Verifying the heaviest tipset to be the genesis"
MSG=$($FOREST_CLI_PATH chain head)
assert_eq "$MSG" $'[\n  "bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4"\n]'

wait_until_p2p_is_ready
ADDRESS=$($FOREST_CLI_PATH net listen | tail -n 1)
echo "Stateless node address: $ADDRESS"
PEER_ID=$(echo "$ADDRESS" | cut --delimiter="/" --fields=7 --zero-terminated)
echo "Stateless node peer id: $PEER_ID"

# Run a normal forest node that only connects to the stateless node
CONFIG_PATH="./forest_config.toml"
cat <<- EOF > $CONFIG_PATH
	[network]
	listening_multiaddrs = ["/ip4/127.0.0.1/tcp/0"]
	bootstrap_peers = ["$ADDRESS"]
EOF

NODE_LOG_DIRECTORY=$(mktemp --directory)
RUST_LOG="info,forest_filecoin::chain_sync::network_context=debug" $FOREST_PATH --chain calibnet --encrypt-keystore false --auto-download-snapshot --config "$CONFIG_PATH" --no-metrics --rpc false --log-dir "$NODE_LOG_DIRECTORY" &
FOREST_NODE_PID=$!
# Verify that the stateless node can respond to chain exchange requests
until grep -r -m 1 "non-empty ChainExchange response from $PEER_ID" "$NODE_LOG_DIRECTORY"; do
    sleep 1s;
done
kill -KILL $FOREST_NODE_PID
