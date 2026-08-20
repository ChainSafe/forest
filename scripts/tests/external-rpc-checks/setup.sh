#!/usr/bin/env bash
# Runs the RPC checks against the external data.riba.plus dataset.
# Needs docker only; everything else happens in containers.

set -euo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
pushd "${PARENT_PATH}"

# This should not be needed in GH. It is useful for running locally.
docker compose down --remove-orphans --volumes

# Imports the snapshot and back-fills the index, recording the head epoch.
docker compose run --rm init
SNAPSHOT_EPOCH="$(docker compose run --rm --no-TTY --entrypoint cat init /data/snapshot-epoch)"
START=$((SNAPSHOT_EPOCH - 1000))
END=$((SNAPSHOT_EPOCH - 1))

docker compose up --detach --wait forest

docker compose run --rm verify "${START}" "${END}"

docker compose run --rm rpc-checks "${START}" "${END}"

popd
