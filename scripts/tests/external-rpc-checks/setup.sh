#!/usr/bin/env bash
# Runs the RPC checks against the external data.riba.plus dataset.
# Needs docker, curl and jq; the snapshot is downloaded into a docker volume.

set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
pushd "${PARENT_PATH}"

RPC_URL="http://127.0.0.1:2345/rpc/v1"

# Confirms the back-fill indexed an epoch, which is what the checks query.
verify_epoch_indexed() {
  curl --silent --show-error --fail --retry 5 --retry-delay 5 --connect-timeout 10 \
    --header 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getBlockByNumber\",\"params\":[\"$(printf '0x%x' "$1")\",false]}" \
    "${RPC_URL}" | jq -e '.result.number' > /dev/null
}

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans --volumes

# Imports the snapshot and back-fills the index, recording the head epoch.
docker compose run --rm init
SNAPSHOT_EPOCH="$(docker compose run --rm --no-TTY --entrypoint cat init /data/snapshot-epoch)"
START=$((SNAPSHOT_EPOCH - 1000))
END=$((SNAPSHOT_EPOCH - 1))

docker compose up --detach --wait forest

verify_epoch_indexed "${START}"
verify_epoch_indexed "${END}"

docker compose run --rm rpc-checks "${START}" "${END}"

popd
