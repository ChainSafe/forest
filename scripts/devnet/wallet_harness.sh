#!/bin/bash
# Helpers for running the wallet/mpool integration suite against the local
# docker devnet. Meant to be sourced (not executed) after the devnet is up
# (see `setup.sh`) and synced (see `check.sh`).
#
# The wallet tests are chain-agnostic and only need:
# - `FULLNODE_API_INFO`            -> RPC endpoint + admin token
# - `FOREST_TEST_PRELOADED_ADDRESS`-> a funded sender address
#
# On the devnet, the funded sender is the genesis miner owner, whose key lives
# in the shared lotus volume at
# `${LOTUS_DATA_DIR}/genesis-sectors/pre-seal-${MINER_ACTOR_ADDRESS}.key`.

# Path to the directory containing this script.
WALLET_HARNESS_PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
source "${WALLET_HARNESS_PARENT_PATH}/.env"

# Allow overriding the binaries, mirroring `scripts/tests/harness.sh`.
export FOREST_CLI_PATH="${FOREST_CLI_PATH:-forest-cli}"
export FOREST_WALLET_PATH="${FOREST_WALLET_PATH:-forest-wallet}"

# Wire up the host environment so `forest-cli`/`forest-wallet`/`forest-dev`
# talk to the dockerized Forest node and have access to the funded genesis key.
function devnet_wallet_env_init {
  set -euo pipefail

  # Admin token is written by the Forest container on startup.
  local token
  token=$(docker exec forest cat "${FOREST_DATA_DIR}/token.jwt")
  export FULLNODE_API_INFO="${token}:/ip4/127.0.0.1/tcp/${FOREST_RPC_PORT}/http"

  # Extract the funded genesis key from the running container (it mounts the
  # shared lotus volume) onto the host so we can import it locally.
  local key_path="${TMPDIR:-/tmp}/devnet_preloaded_wallet.key"
  docker cp "forest:${LOTUS_DATA_DIR}/genesis-sectors/pre-seal-${MINER_ACTOR_ADDRESS}.key" "${key_path}"

  # Import into both keystores (local file + node-managed remote) so the
  # `Backend::Local` and `Backend::Remote` test variants both work.
  FOREST_TEST_PRELOADED_ADDRESS="$(${FOREST_WALLET_PATH} import "${key_path}")"
  export FOREST_TEST_PRELOADED_ADDRESS
  ${FOREST_WALLET_PATH} --remote-wallet import "${key_path}" || true

  echo "Devnet wallet env initialised:"
  echo "  FULLNODE_API_INFO=<token>:/ip4/127.0.0.1/tcp/${FOREST_RPC_PORT}/http"
  echo "  FOREST_TEST_PRELOADED_ADDRESS=${FOREST_TEST_PRELOADED_ADDRESS}"

  # Sanity checks: the node is reachable and the preloaded address is funded.
  ${FOREST_CLI_PATH} chain head
  ${FOREST_WALLET_PATH} --remote-wallet balance "${FOREST_TEST_PRELOADED_ADDRESS}" --exact-balance
}
