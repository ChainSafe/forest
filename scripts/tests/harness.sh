#!/usr/bin/env bash
# This file contains the functions and definitions for
# the Forest tests. It is meant to be sourced by other scripts and not
# executed directly.

export FOREST_CHAIN_INDEXER_ENABLED="1"

export FOREST_PATH="forest"
export FOREST_CLI_PATH="forest-cli"
export FOREST_WALLET_PATH="forest-wallet"
export FOREST_TOOL_PATH="forest-tool"

TMP_DIR=$(mktemp --directory)
LOG_DIRECTORY=$TMP_DIR/logs

export TMP_DIR
export LOG_DIRECTORY

function forest_import_non_calibnet_snapshot {
  echo "Importing a non calibnet snapshot"
  $FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --import-snapshot ./test-snapshots/chain4.car
}

function forest_download_and_import_snapshot {
  echo "Downloading and importing snapshot"
  $FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --auto-download-snapshot
}

function get_epoch_from_car_db {
  DB_PATH=$($FOREST_TOOL_PATH db stats --chain calibnet | grep "Database path:" | cut -d':' -f2- | xargs)
  SNAPSHOT=$(ls "$DB_PATH/car_db"/*.car.zst)
  forest_query_epoch "$SNAPSHOT"
}

function backfill_db {
  echo "Backfill db"

  local snapshot_epoch
  snapshot_epoch=$(get_epoch_from_car_db)
  echo "Snapshot epoch: $snapshot_epoch"

  # Return an error if no argument is provided
  if [[ -z "$1" ]]; then
    echo "Error: No argument provided. Please provide the backfill epochs."
    return 1
  fi

  # Use the provided argument for backfill epochs
  local backfill_epochs
  backfill_epochs=$1

  $FOREST_TOOL_PATH index backfill --chain calibnet --from "$snapshot_epoch" --n-tipsets "$backfill_epochs"
}

function forest_check_db_stats {
  echo "Checking DB stats"
  $FOREST_TOOL_PATH db stats --chain calibnet
}

function forest_query_epoch {
  $FOREST_TOOL_PATH archive info "$1" | grep Epoch | awk '{print $2}'
}

function forest_query_state_roots {
  $FOREST_TOOL_PATH archive info "$1" | grep State-roots | awk '{print $2}'
}

function forest_query_format {
  $FOREST_TOOL_PATH archive info "$1" | grep "CAR format" | awk '{print $3}'
}

function forest_run_node_detached {
  echo "Running forest"
  /usr/bin/time -v $FOREST_PATH --chain calibnet --encrypt-keystore false --log-dir "$LOG_DIRECTORY" &
}

function forest_run_node_stateless_detached {
  CONFIG_PATH="./stateless_forest_config.toml"
  echo "${CONFIG_PATH}"
  echo "Running forest in stateless and detached mode"
  cat <<- EOF > $CONFIG_PATH
		[client]
		data_dir = "/tmp/stateless_forest_data"

		[network]
		listening_multiaddrs = ["/ip4/127.0.0.1/tcp/0"]
	EOF

  $FOREST_PATH --chain calibnet --encrypt-keystore false --config "$CONFIG_PATH" --log-dir "$LOG_DIRECTORY" --save-token ./stateless_admin_token --stateless &
}

function forest_wait_api {
  echo "Waiting for Forest API"
  $FOREST_CLI_PATH wait-api --timeout 60s
}

function forest_wait_for_sync {
  echo "Waiting for sync"
  timeout 30m $FOREST_CLI_PATH sync wait
}

function forest_wait_for_healthcheck_ready {
  echo "Waiting for healthcheck ready"
  timeout 30m $FOREST_CLI_PATH healthcheck ready --wait
}

function forest_init {
  forest_download_and_import_snapshot

  if [[ "${1:-}" == "--backfill-db" ]]; then
    if [[ "${2:-}" =~ ^[0-9]+$ ]]; then
      backfill_db "$2"
    else
      echo "Error: Expected a numeric argument after --backfill-db"
      return 1
    fi
  fi

  forest_check_db_stats
  forest_run_node_detached

  forest_wait_api
  forest_wait_for_sync
  forest_check_db_stats

  DATA_DIR=$( $FOREST_CLI_PATH config dump | grep "data_dir" | cut -d' ' -f3- | tr -d '"' )
  ADMIN_TOKEN=$(cat "${DATA_DIR}/token")
  FULLNODE_API_INFO="${ADMIN_TOKEN}:/ip4/127.0.0.1/tcp/2345/http"

  export FULLNODE_API_INFO
}

function forest_init_stateless {
  forest_run_node_stateless_detached
  forest_wait_api
  
  ADMIN_TOKEN=$(cat stateless_admin_token)
  FULLNODE_API_INFO="$ADMIN_TOKEN:/ip4/127.0.0.1/tcp/2345/http"

  export ADMIN_TOKEN
  export FULLNODE_API_INFO
}

function forest_print_logs_and_metrics {
  echo "Get and print metrics"
  wget -O metrics.log http://localhost:6116/metrics

  echo "--- Forest Prometheus metrics ---"; cat metrics.log
  echo "Print forest log files"
  ls -hl "$LOG_DIRECTORY"
  cat "$LOG_DIRECTORY"/*
}

function forest_cleanup {
  if pkill -0 forest 2>/dev/null; then
    forest_print_logs_and_metrics
    $FOREST_CLI_PATH shutdown --force || true
    timeout 10s sh -c "while pkill -0 forest 2>/dev/null; do sleep 1; done"
  fi
}

function assert_eq {
  local expected="$1"
  local actual="$2"
  local msg="${3-}"

  if [ "$expected" == "$actual" ]; then
    return 0
  else
    [ "${#msg}" -gt 0 ] && echo "$expected == $actual :: $msg"
    return 1
  fi
}

trap forest_cleanup EXIT
