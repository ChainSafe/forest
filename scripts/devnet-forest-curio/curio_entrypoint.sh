#!/bin/bash
# Bootstrap Curio against Forest for genesis ${MINER_ACTOR_ADDRESS}, then run.
#
# Storage attach needs the live Curio API, so we start Curio first, cordon it so
# sealing/mining does not race ahead, attach genesis + seal storage (idempotent
# across restart:on-failure), then uncordon. `.init.ready` marks a healthy node
# for the compose healthcheck.
set -euo pipefail

set +x
FULLNODE_API_INFO="$(cat "${FOREST_DATA_DIR}/token.jwt"):/dns/forest/tcp/${FOREST_RPC_PORT}/http"
export FULLNODE_API_INFO
set -x

ADDR="$(hostname -I | tr ' ' '\n' | awk '$1 ~ /^[0-9]+\./ && $1 !~ /^127\./ {print $1; exit}')"
if [ -z "${ADDR}" ]; then
  echo "failed to resolve container IPv4 for Curio --machine" >&2
  exit 1
fi
MACHINE="${ADDR}:12300"

LAYER_HASH="$(sha256sum /devnet-layer.toml | awk '{print $1}')"
if [ ! -f "${CURIO_REPO_PATH}/.init.config" ]; then
  curio config new-cluster "${MINER_ACTOR_ADDRESS}"
  curio config create --title devnet < /devnet-layer.toml
  echo "${LAYER_HASH}" > "${CURIO_REPO_PATH}/.init.layer"
  touch "${CURIO_REPO_PATH}/.init.config"
elif [ "$(cat "${CURIO_REPO_PATH}/.init.layer" 2>/dev/null || true)" != "${LAYER_HASH}" ]; then
  echo "devnet-layer.toml changed since this curio-data volume was init'd; re-run setup.sh (it wipes curio-data)" >&2
  exit 1
fi

# Drop a stale ready marker until this boot finishes attach + uncordon.
rm -f "${CURIO_REPO_PATH}/.init.ready"

CURIO_PID=""
cleanup() {
  if [ -n "${CURIO_PID}" ] && kill -0 "${CURIO_PID}" 2>/dev/null; then
    kill "${CURIO_PID}" 2>/dev/null || true
    wait "${CURIO_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT TERM INT

curio run --nosync --name forest-devnet --layers devnet &
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

# Hold scheduling until storage is ready (API is up, attach is next).
curio cli --machine "${MACHINE}" cordon

# Always try to leave the machine schedulable if attach fails mid-boot.
uncordon() {
  curio cli --machine "${MACHINE}" uncordon || true
}

storage_path_listed() {
  local path=$1
  local list
  list=$(curio cli --machine "${MACHINE}" storage list 2>/dev/null) || return 1
  printf '%s\n' "${list}" | grep -qF -- "${path}"
}

# A crash after --init leaves the path unlisted; retry without flags.
attach_storage() {
  local label=$1
  shift
  local out rc path
  path=$(printf '%s\n' "$@" | awk '/^\// {p=$0} END {print p}')
  if [ -z "${path}" ]; then
    echo "storage attach ${label}: no filesystem path" >&2
    return 1
  fi

  set +e
  out=$(curio cli --machine "${MACHINE}" storage attach "$@" 2>&1)
  rc=$?
  set -e
  if [ "${rc}" -eq 0 ]; then
    echo "attached ${label}"
    return 0
  fi
  if storage_path_listed "${path}"; then
    echo "storage already listed (${label}: ${path}); continuing"
    return 0
  fi
  if printf '%s\n' "${out}" | grep -q 'path is already initialized'; then
    echo "storage metadata present (${label}); attaching without --init"
    set +e
    out=$(curio cli --machine "${MACHINE}" storage attach "${path}" 2>&1)
    rc=$?
    set -e
    if [ "${rc}" -eq 0 ] || storage_path_listed "${path}"; then
      echo "attached ${label}"
      return 0
    fi
  fi
  echo "${out}" >&2
  return "${rc}"
}

if [ ! -f "${CURIO_REPO_PATH}/.init.storage" ]; then
  # --store marks the pre-sealed replicas primary for Winning PoSt.
  if ! attach_storage genesis-sectors --init --store "${LOTUS_DATA_DIR}/genesis-sectors"; then
    uncordon
    exit 1
  fi
  if ! attach_storage seal-store --init --seal --store "${CURIO_REPO_PATH}/storage"; then
    uncordon
    exit 1
  fi
  touch "${CURIO_REPO_PATH}/.init.storage"
fi

curio cli --machine "${MACHINE}" uncordon
touch "${CURIO_REPO_PATH}/.init.ready"

# Disarm EXIT trap so a clean `wait` exit does not re-kill a dead PID noisily.
trap - EXIT
trap cleanup TERM INT
wait "${CURIO_PID}" || exit 1
