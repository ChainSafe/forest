#!/usr/bin/env bash
# Runs the RPC checks against the external data.riba.plus dataset.
# Needs docker only; everything else happens in containers.

set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
pushd "${PARENT_PATH}"

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans --volumes

# Imports the snapshot and back-fills the index, recording the range it covered.
docker compose run --rm init
CHECK_RANGE="$(docker compose run --rm --no-TTY --entrypoint cat init /data/check-range)"
read -r START END <<< "${CHECK_RANGE}"
[[ ${START} =~ ^[0-9]+$ && ${END} =~ ^[0-9]+$ ]] || {
  echo "init did not report a usable epoch range: ${CHECK_RANGE}"
  exit 1
}

docker compose up --detach --wait forest

docker compose run --rm verify "${START}" "${END}"

docker compose run --rm rpc-checks "${START}" "${END}"

popd
