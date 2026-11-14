#!/bin/bash
# Sourced: Forest RPC wait + Forest/Lotus past TARGET_HEIGHT.

# jq exits 0 on empty input; do not map a failed RPC to height 0.
function get_sync_height {
  local port=$1
  local height
  height=$(curl --silent --max-time 2 -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","params":null}' \
       "http://127.0.0.1:${port}/rpc/v1" | jq -r '.result.Height // empty') || return 1
  if [ -z "${height}" ]; then
    return 1
  fi
  printf '%s\n' "${height}"
}

function wait_forest_rpc {
  until curl --silent --max-time 2 -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","params":null}' \
       "http://127.0.0.1:${FOREST_RPC_PORT}/rpc/v1" \
       >/dev/null; do
    echo "Forest is unavailable - sleeping for 1s"
    sleep 1
  done
}

# SCRIPT_TIMEOUT_MINUTES (default 15); leave 60s so we fail before the GHA killer.
function check_timeout_secs {
  local timeout_secs=$(( ${SCRIPT_TIMEOUT_MINUTES:-15} * 60 ))
  if [ "${timeout_secs}" -gt 60 ]; then
    timeout_secs=$((timeout_secs - 60))
  fi
  echo "${timeout_secs}"
}

function wait_forest_and_lotus_past_target {
  local success_msg=$1
  local timeout_secs=$2
  local start_time timeout target_height forest_height lotus_height current_time
  start_time=$(date +%s)
  timeout=$((start_time + timeout_secs))
  target_height=$TARGET_HEIGHT

  while true; do
    forest_height=$(get_sync_height "${FOREST_RPC_PORT}") || true
    lotus_height=$(get_sync_height "${LOTUS_RPC_PORT}") || true
    if [[ "${forest_height}" =~ ^[0-9]+$ && "${lotus_height}" =~ ^[0-9]+$ ]] \
      && [ "${forest_height}" -gt "${target_height}" ] \
      && [ "${lotus_height}" -gt "${target_height}" ]; then
      echo "Both nodes past $target_height (Forest: $forest_height, Lotus: $lotus_height): ${success_msg}"
      return 0
    fi

    current_time=$(date +%s)
    if [ "$current_time" -gt "$timeout" ]; then
      echo "Timeout reached (Forest: $forest_height, Lotus: $lotus_height), target $target_height not reached by both" >&2
      return 1
    fi

    sleep 1
  done
}
