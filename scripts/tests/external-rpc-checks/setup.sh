#!/usr/bin/env bash
# Runs the RPC checks against the external data.riba.plus dataset.
# Needs docker only; everything else happens in containers.

set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
pushd "${PARENT_PATH}"

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans --volumes

# The dataset publishes each UTC day's archives some time the next morning, so
# yesterday's may not exist yet. Try yesterday's snapshot first, and the day
# before if the dataset has no data for it.
for days_ago in 1 2; do
  docker compose run --rm --env DAYS_AGO="${days_ago}" resolve
  CHECK_RANGE="$(docker compose run --rm --no-TTY --entrypoint cat resolve /data/check-range)"
  read -r START END <<< "${CHECK_RANGE}"
  [[ ${START} =~ ^[0-9]+$ && ${END} =~ ^[0-9]+$ ]] || {
    echo "resolve did not report a usable epoch range: ${CHECK_RANGE}"
    exit 1
  }

  probe=0
  docker compose run --rm rpc-checks --probe --network "${FOREST_CHAIN}" "${START}" "${END}" || probe=$?
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
docker compose run --rm init

docker compose up --detach --wait forest

docker compose run --rm verify "${START}" "${END}"

docker compose run --rm rpc-checks "${START}" "${END}"

popd
