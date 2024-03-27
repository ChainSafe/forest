#!/usr/bin/env bash
# This script is used to test the offline RPC API server against itself.
# It's run in CI, and uses forest-tool api compare subcommand to test RPC endpoints.

set -euxo pipefail

FOREST_TOOL_PATH="forest-tool"
PORTS=(8080 8081)

# Function to stop services on specified ports
stop_services() {
    for port in "${PORTS[@]}"; do
        fuser -k "$port/tcp" || true
    done
    # Remove downloaded snapshot file
    rm -rf "$snapshot"
}
trap stop_services EXIT

old_snapshot=forest_diff_calibnet_2022-11-02_height_0+3000.forest.car.zst
curl --location --remote-name "https://forest-archive.chainsafe.dev/calibnet/diff/$old_snapshot"

# Fetch latest calibnet snapshot 
snapshot="$("$FOREST_TOOL_PATH" snapshot fetch --chain calibnet)"

# Start Offline RPC servers on ports
for i in "${!PORTS[@]}"; do
  port=${PORTS[$i]}
  "$FOREST_TOOL_PATH" api serve "$snapshot" "$old_snapshot" --chain calibnet --port "$port" &
done

# Check if services on ports have started
for port in "${PORTS[@]}"; do
    until nc -z localhost "$port"; do
        sleep 30
    done
done

for port in "${PORTS[@]}"; do
  # Assert an old block is present, given that the old snapshot is used.
  # https://calibration.filfox.info/en/block/bafy2bzacecpjvcld56dazyukvj35uzwvlh3tb4ga2lvbgbiua3mgbqaz45hbm
  temp_dir=$(mktemp -d)
  FULLNODE_API_INFO="/ip4/127.0.0.1/tcp/$port/http" forest-cli chain block -c bafy2bzacecpjvcld56dazyukvj35uzwvlh3tb4ga2lvbgbiua3mgbqaz45hbm | jq . > "$temp_dir/block.json"

  # assert block is as expected
  parent_path=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
  diff "$parent_path/test_data/calibnet_block_3000.json" "$temp_dir/block.json"
done

# TODO(aatifsyed): https://github.com/ChainSafe/forest/pull/4096
#                  `--filter` logic should be commonised
# Compare the http endpoints
$FOREST_TOOL_PATH api compare "$snapshot" --forest /ip4/127.0.0.1/tcp/8080/http --lotus /ip4/127.0.0.1/tcp/8081/http --n-tipsets 5 '--filter=!Filecoin.StateWaitMsg'

# Compare the ws endpoints
$FOREST_TOOL_PATH api compare "$snapshot" --forest /ip4/127.0.0.1/tcp/8080/ws --lotus /ip4/127.0.0.1/tcp/8081/ws --n-tipsets 5 '--filter=!Filecoin.StateWaitMsg'
