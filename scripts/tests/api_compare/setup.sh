#!/bin/bash
# This script is used to set up clean environment for the
# API compare checks.

set -euxo pipefail

# Path to the directory containing this script.
PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
pushd "${PARENT_PATH}"
source .env

# This should not be needed in GH. It is useful for running locally.
docker compose --profile offline-rpc-server down --remove-orphans
docker compose rm -f
# Cleanup data volumes
# docker volume rm -f api_compare_node-data

# Run it in the background so we can perform checks on it.
# Ideally, we could use `--wait` and `--wait-timeout` to wait for services
# to be up. However, `compose` does not distinct between services and 
# init containers. See more: https://github.com/docker/compose/issues/10596
docker compose --profile offline-rpc-server up --build --force-recreate --detach --timestamps

popd
