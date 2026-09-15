#!/usr/bin/env bash
# Runs the RPC checks against the external data.riba.plus dataset.
# Needs docker only; everything else happens in containers.

set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
pushd "${PARENT_PATH}"

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans --volumes

# The dataset publishes a UTC day's archives the morning after, at no fixed
# time, so test the newest day it has published: yesterday, else the day
# before. The checks cover the 1000 epochs below the snapshot's head epoch.
for days_ago in 1 2; do
  docker compose run --rm --env DAYS_AGO="${days_ago}" init resolve
  SNAPSHOT_EPOCH="$(docker compose run --rm --no-TTY --entrypoint cat init /data/snapshot-epoch)"
  START=$((SNAPSHOT_EPOCH - 1000))
  END=$((SNAPSHOT_EPOCH - 1))

  probe=0
  docker compose run --rm rpc-checks --probe --network calibnet "${START}" "${END}" || probe=$?
  case ${probe} in
    0) break ;;
    2) echo "the dataset has not published that day yet" ;;
    *) exit "${probe}" ;;
  esac
done
[[ ${probe} -eq 0 ]] || {
  echo "the dataset has published neither yesterday nor the day before; raise it with its maintainer"
  exit 1
}

# Imports the snapshot and back-fills the index.
docker compose run --rm init import

docker compose up --detach --wait forest

docker compose run --rm verify "${START}" "${END}"

docker compose run --rm rpc-checks "${START}" "${END}"

popd
