#!/bin/bash
# Build the Curio image if needed, bring up the stack, wait for Forest's RPC.

set -euxo pipefail

PARENT_PATH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
pushd "${PARENT_PATH}"
# shellcheck source=/dev/null
source .env

# Upgrade heights must stay in lockstep with the Lotus Miner stack.
miner_env="${PARENT_PATH}/../devnet-forest-miner/.env"
while IFS='=' read -r key value; do
  case "${key}" in
    GENESIS_NETWORK_VERSION|*_HEIGHT)
      ours="${!key-}"
      if [ "${ours}" != "${value}" ]; then
        echo "ERROR: ${key}=${ours} differs from ${miner_env} (${value})" >&2
        exit 1
      fi
      ;;
  esac
done < <(grep -E '^(GENESIS_NETWORK_VERSION|[A-Z_]+_HEIGHT)=' "${miner_env}")

./build_image.sh

docker compose down --remove-orphans
docker compose rm -f
docker volume rm -f \
  devnet-forest-curio_lotus-data \
  devnet-forest-curio_forest-data \
  devnet-forest-curio_curio-data \
  devnet-forest-curio_yugabyte-data

docker compose up --build --force-recreate --detach

function call_forest_chain_head {
  curl --silent -X POST -H "Content-Type: application/json" \
       --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.ChainHead","param":"null"}' \
       "http://127.0.0.1:${FOREST_RPC_PORT}/rpc/v1"
}

until call_forest_chain_head; do
  echo "Forest is unavailable - sleeping for 1s"
  sleep 1
done

popd
