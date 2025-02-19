#!/bin/bash
set -euxo pipefail

# This script tests Forest is able to catch up the butterflynet.

source "$(dirname "$0")/harness.sh"

function shutdown {
  kill -KILL $FOREST_NODE_PID
}

trap shutdown EXIT

$FOREST_PATH --chain butterflynet --encrypt-keystore false &
FOREST_NODE_PID=$!

forest_wait_api

forest_wait_for_sync
