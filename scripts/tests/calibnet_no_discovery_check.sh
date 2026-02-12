#!/usr/bin/env bash
set -euxo pipefail

# This script tests forest behaviours when discovery(mdns and kademlia) is disabled

source "$(dirname "$0")/harness.sh"

function shutdown {
  kill -KILL $FOREST_NODE_PID
}

trap shutdown EXIT

$FOREST_PATH --chain calibnet --encrypt-keystore false --mdns false --kademlia false --auto-download-snapshot --exit-after-init
$FOREST_PATH --chain calibnet --encrypt-keystore false --mdns false --kademlia false --auto-download-snapshot --log-dir "$LOG_DIRECTORY" &
FOREST_NODE_PID=$!

forest_wait_api

# Verify that one of the seed nodes has been connected to
until $FOREST_CLI_PATH net peers | grep "calib"; do
    sleep 1s;
done

# Verify F3 is getting certificates from the network
until [[ $($FOREST_CLI_PATH f3 certs get --output json | jq '.GPBFTInstance') -gt 50 ]]; do
    sleep 1s;
done

echo "Test subcommands: f3 status"
$FOREST_CLI_PATH f3 status
echo "Test subcommands: f3 manifest"
$FOREST_CLI_PATH f3 manifest
echo "Test subcommands: f3 certs list"
$FOREST_CLI_PATH f3 certs list
echo "Test subcommands: f3 certs get"
$FOREST_CLI_PATH f3 certs get
