#!/bin/bash
# This script is used to set up clean environment for the bootstrapper tests.

set -euxo pipefail

# Accepts one arg : Forest or Lotus
if [ $# -ne 1 ]; then
    echo "Usage: $0 <forest|lotus>"
    exit 1
fi

if [ "$1" == "forest" ]; then
    COMPOSE_FILE="docker-compose-forest.yml"
elif [ "$1" == "lotus" ]; then
    COMPOSE_FILE="docker-compose-lotus.yml"
else
    echo "Usage: $0 <Forest|Lotus>"
    exit 1
fi

# Path to the directory containing this script.
PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
pushd "${PARENT_PATH}"
source .env

# This should not be needed in GH. It is useful for running locally.
docker compose -f $COMPOSE_FILE down --remove-orphans
docker compose -f $COMPOSE_FILE rm -f

# Run it in the background so we can perform checks on it.
# Ideally, we could use `--wait` and `--wait-timeout` to wait for services
# to be up. However, `compose` does not distinct between services and 
# init containers. See more: https://github.com/docker/compose/issues/10596
docker compose -f $COMPOSE_FILE up --build --force-recreate --detach --timestamps

popd
