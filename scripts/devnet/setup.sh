#!/bin/bash
# This script is used to set up the CI environment for the
# local devnet tests.

set -euxo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
pushd "${PARENT_PATH}"
# shellcheck source=/dev/null
source .env
# shellcheck source=/dev/null
source "${PARENT_PATH}/chain_height.sh"

# This should not be needed in GH. It is useful for running locally.
docker compose --profile "*" down --remove-orphans
docker compose rm -f
# Cleanup data volumes
docker volume rm -f devnet_lotus-data
docker volume rm -f devnet_forest-data

# Run it in the background so we can perform checks on it.
# Ideally, we could use `--wait` and `--wait-timeout` to wait for services
# to be up. However, `compose` does not distinct between services and
# init containers. See more: https://github.com/docker/compose/issues/10596
docker compose up --build --force-recreate --detach

wait_forest_rpc

popd
