#!/usr/bin/env bash
# This script is used to test both online and offline RPC API server against lotus.
# It runs forest daemon, forest offline rpc server and lotus 
# and uses forest-tool api compare subcommand to test RPC endpoints.

set -euxo pipefail

pushd "$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )"
LOTUS_IMAGE="filecoin/lotus-all-in-one:v1.26.0-rc3-calibnet"
FOREST_PATH="forest"
FOREST_CLI_PATH="forest-cli"
FOREST_TOOL_PATH="forest-tool"
PORTS=(1234 2345 3456)

# Function to stop services on specified ports
stop_services() {
    kill -KILL "$FOREST_PID" || true
    kill -KILL "$FOREST_TOOL_PID" || true
    kill -KILL "$LOTUS_PID" || true
    docker container rm -f lotus || true
    for port in "${PORTS[@]}"; do
        fuser -k "$port/tcp" || true
    done
}
trap stop_services EXIT

# Run forest daemon RPC server at 2345
"$FOREST_PATH" --chain calibnet --encrypt-keystore false --no-gc --height=-900 --auto-download-snapshot &
FOREST_PID=$!

"$FOREST_CLI_PATH" sync wait
EXPORTED_SNAPSHOT="latest.forest.car.zst"
"$FOREST_CLI_PATH" snapshot export -o "$EXPORTED_SNAPSHOT" -d=900 --include-message-receipts

old_snapshot=forest_diff_calibnet_2022-11-02_height_0+3000.forest.car.zst
curl --location --remote-name "https://forest-archive.chainsafe.dev/calibnet/diff/$old_snapshot"

# Run forest-tool RPC server at 3456
"$FOREST_TOOL_PATH" api serve "$EXPORTED_SNAPSHOT" "$old_snapshot" --chain calibnet --port 3456 &
FOREST_TOOL_PID=$!

# Run lotus daemon RPC server at 1234
docker run --name lotus --rm \
  -e LOTUS_API_LISTENADDRESS=/ip4/0.0.0.0/tcp/1234/http \
  -e LOTUS_FEVM_ENABLEETHRPC=1 \
  -e LOTUS_CHAINSTORE_ENABLESPLITSTORE=false \
  -p 1234:1234 \
  -v "$PWD:/pwd" \
  -v /var/tmp/filecoin-proof-parameters:/var/tmp/filecoin-proof-parameters \
  "$LOTUS_IMAGE" lotus daemon --remove-existing-chain --import-snapshot "/pwd/$EXPORTED_SNAPSHOT" &
LOTUS_PID=$!

# Check if services on ports have started
for port in "${PORTS[@]}"; do
    until nc -z localhost "$port"; do
        sleep 10
    done
done

until docker exec lotus lotus sync wait; do
    sleep 5
done

# Assert an old block is present, given that the old snapshot is used.
# https://calibration.filfox.info/en/block/bafy2bzacecpjvcld56dazyukvj35uzwvlh3tb4ga2lvbgbiua3mgbqaz45hbm
temp_dir=$(mktemp -d)
FULLNODE_API_INFO="/ip4/127.0.0.1/tcp/3456/http" forest-cli chain block -c bafy2bzacecpjvcld56dazyukvj35uzwvlh3tb4ga2lvbgbiua3mgbqaz45hbm | jq . > "$temp_dir/block.json"

# assert block is as expected
diff "test_data/calibnet_block_3000.json" "$temp_dir/block.json"

# Compare the http endpoints
# forest daemon vs lotus daemon
$FOREST_TOOL_PATH api compare "$EXPORTED_SNAPSHOT" --forest /ip4/127.0.0.1/tcp/2345/http --lotus /ip4/127.0.0.1/tcp/1234/http --n-tipsets 5 --filter-file api_compare/filter-list
# forest-tool vs lotus daemon
$FOREST_TOOL_PATH api compare "$EXPORTED_SNAPSHOT" --forest /ip4/127.0.0.1/tcp/3456/http --lotus /ip4/127.0.0.1/tcp/1234/http --n-tipsets 5 --filter-file api_compare/filter-list || true

# Compare the ws endpoints
# forest daemon vs lotus daemon
$FOREST_TOOL_PATH api compare "$EXPORTED_SNAPSHOT" --forest /ip4/127.0.0.1/tcp/2345/ws --lotus /ip4/127.0.0.1/tcp/1234/ws --n-tipsets 5 --filter-file api_compare/filter-list || true
# forest-tool vs lotus daemon
$FOREST_TOOL_PATH api compare "$EXPORTED_SNAPSHOT" --forest /ip4/127.0.0.1/tcp/3456/ws --lotus /ip4/127.0.0.1/tcp/1234/ws --n-tipsets 5 --filter-file api_compare/filter-list || true

popd
