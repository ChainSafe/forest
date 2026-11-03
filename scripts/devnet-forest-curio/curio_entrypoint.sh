#!/bin/bash
# Bootstrap Curio against Forest for genesis ${MINER_ACTOR_ADDRESS}, then run.
set -euo pipefail

set +x
export FULLNODE_API_INFO="$(cat "${FOREST_DATA_DIR}/token.jwt"):/dns/forest/tcp/${FOREST_RPC_PORT}/http"
set -x

MACHINE="$(hostname -i | awk '{print $1}'):12300"

if [ ! -f "${CURIO_REPO_PATH}/.init.config" ]; then
  curio config new-cluster "${MINER_ACTOR_ADDRESS}"
  curio config create --title devnet < /devnet-layer.toml
  touch "${CURIO_REPO_PATH}/.init.config"
fi

curio run --nosync --name forest-devnet --layers seal,post,devnet &
CURIO_PID=$!

until curio cli --machine "${MACHINE}" wait-api; do
  if ! kill -0 "${CURIO_PID}" 2>/dev/null; then
    echo "curio exited before its API came up" >&2
    wait "${CURIO_PID}"
    exit 1
  fi
  echo "waiting for curio API on ${MACHINE} ..."
  sleep 2
done

if [ ! -f "${CURIO_REPO_PATH}/.init.storage" ]; then
  curio cli --machine "${MACHINE}" storage attach "${LOTUS_DATA_DIR}/genesis-sectors"
  curio cli --machine "${MACHINE}" storage attach --init --seal --store "${CURIO_REPO_PATH}/storage"
  touch "${CURIO_REPO_PATH}/.init.storage"
fi

wait "${CURIO_PID}"
