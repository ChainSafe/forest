#!/usr/bin/env bash
# Runs the RPC checks against the external data.riba.plus dataset.
# Needs docker, curl and jq; the snapshot is downloaded into a docker volume.

set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
pushd "${PARENT_PATH}"

ARCHIVE_URL="https://forest-archive.chainsafe.dev/list/calibnet/latest-v2?format=json"
RPC_URL="http://127.0.0.1:2345/rpc/v1"

# GNU and BSD `date` spell date arithmetic differently, only GNU has `--version`.
utc_day_offset() {
  if date --version > /dev/null 2>&1; then
    date -u -d "$1 days ago" +%F
  else
    date -u -v-"$1"d +%F
  fi
}

# The dataset lags the chain, so yesterday's snapshot is the one to test against.
day=$(utc_day_offset 1)
url=$(curl --silent --show-error --fail --retry 3 --connect-timeout 10 --max-time 60 "${ARCHIVE_URL}" |
  jq --raw-output --arg day "${day}" '[.items[].url | select(contains("_" + $day + "_"))] | first')
[[ ${url} == https* ]] || {
  echo "no calibnet snapshot published for ${day}"
  exit 1
}

# Snapshot names end in the epoch of their head tipset.
epoch=${url##*_height_}
epoch=${epoch%%.*}
export SNAPSHOT_EPOCH="${epoch}"
START=$((epoch - 1000))
END=$((epoch - 1))

# Confirms the back-fill indexed an epoch, which is what the checks query.
verify_epoch_indexed() {
  curl --silent --show-error --fail --retry 5 --retry-delay 5 --connect-timeout 10 \
    --header 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getBlockByNumber\",\"params\":[\"$(printf '0x%x' "$1")\",false]}" \
    "${RPC_URL}" | jq -e '.result.number' > /dev/null
}

export SNAPSHOT_URL="${url}"

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans --volumes

docker compose run --rm snapshot
docker compose up --detach forest

FULLNODE_API_INFO="$(docker compose exec -T forest cat /data/forest-token)"
export FULLNODE_API_INFO="${FULLNODE_API_INFO}:/dns/forest/tcp/2345/http"
docker compose run --rm backfill

verify_epoch_indexed "${START}"
verify_epoch_indexed "${END}"

docker compose run --rm rpc-checks "${START}" "${END}"

popd
