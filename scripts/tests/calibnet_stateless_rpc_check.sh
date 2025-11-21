#!/bin/bash
set -euxo pipefail

# This script tests RPC on a stateless node. This is done to avoid downloading the snapshot for this test and speed up the CI.

source "$(dirname "$0")/harness.sh"

# Run a stateless node with a filter list as an argument.
function forest_run_node_stateless_detached_with_filter_list {
  pkill -9 forest || true
  local filter_list=$1

  $FOREST_PATH --chain calibnet --encrypt-keystore false --log-dir "$LOG_DIRECTORY" --stateless --rpc-filter-list "$filter_list" &
  forest_wait_api
}

# Tests the RPC method `Filecoin.ChainHead` and checks if the status code matches the expected code.
function test_rpc {
  local expected_code=$1

  # Test the RPC, get status code
  status_code=$(curl --silent -X POST -H "Content-Type: application/json" \
             --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","params": [ ] }' \
             "http://127.0.0.1:2345/rpc/v1" | jq '.error.code')

  # check if the expected code is returned
  if [ "$status_code" != "$expected_code" ]; then
    echo "Expected status code $expected_code, got $status_code"
    exit 1
  fi
}

# No filter list - all RPCs are allowed. This is the default behavior.

cat <<- EOF > "$TMP_DIR"/filter-list
# Cthulhu fhtagn
EOF

forest_run_node_stateless_detached_with_filter_list "$TMP_DIR/filter-list"
test_rpc null # null means there is no error

# Filter list with the `ChainHead` RPC disallowed. Should return 403.

cat <<- EOF > "$TMP_DIR"/filter-list
!Filecoin.ChainHead
EOF

forest_run_node_stateless_detached_with_filter_list "$TMP_DIR/filter-list"
test_rpc 403

# Filter list with a single other RPC allowed. `ChainHead` should be disallowed and return 403.
# Note - this method is required for the test harness.
cat <<- EOF > "$TMP_DIR"/filter-list
Filecoin.Shutdown
EOF

forest_run_node_stateless_detached_with_filter_list "$TMP_DIR/filter-list"
test_rpc 403

# Filter list with a single other RPC allowed, along with `ChainHead`. Should succeed.
cat <<- EOF > "$TMP_DIR"/filter-list
Filecoin.Shutdown
Filecoin.ChainHead
EOF

forest_run_node_stateless_detached_with_filter_list "$TMP_DIR/filter-list"
test_rpc null # null means there is no error
