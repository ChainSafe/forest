#!/usr/bin/env bash
set -euxo pipefail

# This script tests forest behaviours when discovery(mdns and kademlia) is disabled

source "$(dirname "$0")/harness.sh"

$FOREST_PATH --chain calibnet --encrypt-keystore false --mdns false --kademlia false --auto-download-snapshot --log-dir "$LOG_DIRECTORY" --detach --save-token ./admin_token
FULLNODE_API_INFO="$(cat admin_token):/ip4/127.0.0.1/tcp/2345/http"
export FULLNODE_API_INFO

wait_util_rpc_is_ready

# Verify that one of the seed nodes has been connected to
until $FOREST_CLI_PATH net peers | grep "bootstrap-0.calibration.fildev.network"; do
    sleep 1s;
done
