#!/bin/bash
# Bring up Forest+Curio. Compose builds the Curio image from curio.dockerfile.

set -euxo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
pushd "${PARENT_PATH}"
set -a
# shellcheck source=/dev/null
source .env
# shellcheck source=/dev/null
source "${PARENT_PATH}/../devnet/upgrade_heights.env"
set +a
# shellcheck source=/dev/null
source "${PARENT_PATH}/../devnet/chain_height.sh"

docker compose down --remove-orphans
docker compose rm -f
docker volume rm -f \
  devnet-forest-curio_lotus-data \
  devnet-forest-curio_forest-data \
  devnet-forest-curio_curio-data \
  devnet-forest-curio_yugabyte-data

# Rebuilds Forest, and Curio when curio.dockerfile or CURIO_COMMIT changed.
docker compose up --build --force-recreate --detach

wait_forest_rpc

until docker exec curio test -f "${CURIO_REPO_PATH}/.init.ready"; do
  # restart:on-failure briefly reports "restarting", not "running".
  status="$(docker inspect -f '{{.State.Status}}' curio 2>/dev/null || echo missing)"
  case "${status}" in
    running|restarting) ;;
    *)
      echo "ERROR: curio container status is '${status}' while waiting for .init.ready" >&2
      docker logs curio >&2 || true
      exit 1
      ;;
  esac
  echo "Curio is not ready - sleeping for 2s"
  sleep 2
done

popd
