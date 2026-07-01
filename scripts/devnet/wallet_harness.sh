#!/bin/bash
# Sourced (not executed) helpers for the wallet/mpool suite on the docker
# devnet. Run after the devnet is up (`setup.sh`) and synced (`check.sh`).
#
# The genesis key is the Lotus miner's default wallet, so using it as the test
# sender causes nonce contention. We fund a dedicated wallet instead.

WALLET_HARNESS_PARENT_PATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
source "${WALLET_HARNESS_PARENT_PATH}/.env"

export FOREST_CLI_PATH="${FOREST_CLI_PATH:-forest-cli}"
export FOREST_WALLET_PATH="${FOREST_WALLET_PATH:-forest-wallet}"
export DEVNET_TEST_FUND_AMT="${DEVNET_TEST_FUND_AMT:-100 FIL}"

function devnet_wallet_env_init {
  set -euo pipefail

  local token
  token=$(docker exec forest cat "${FOREST_DATA_DIR}/token.jwt")
  export FULLNODE_API_INFO="${token}:/ip4/127.0.0.1/tcp/${FOREST_RPC_PORT}/http"

  # Derive the genesis address via a throwaway keystore (`XDG_DATA_HOME`) so it
  # never lands in the real keystores.
  local genesis_key_path="${TMPDIR:-/tmp}/devnet_genesis_wallet.key"
  docker cp "forest:${LOTUS_DATA_DIR}/genesis-sectors/pre-seal-${MINER_ACTOR_ADDRESS}.key" "${genesis_key_path}"
  local genesis_addr
  genesis_addr="$(XDG_DATA_HOME="$(mktemp -d)" ${FOREST_WALLET_PATH} import "${genesis_key_path}")"

  # Fresh sender the miner never touches; mirror to the remote keystore so both
  # `Backend::Local` and `Backend::Remote` variants work.
  local test_addr test_key_path
  test_addr="$(${FOREST_WALLET_PATH} new)"
  test_key_path="${TMPDIR:-/tmp}/devnet_test_wallet.key"
  ${FOREST_WALLET_PATH} export "${test_addr}" > "${test_key_path}"
  ${FOREST_WALLET_PATH} --remote-wallet import "${test_key_path}"
  export FOREST_TEST_PRELOADED_ADDRESS="${test_addr}"

  # Fund it from the genesis key (the node holds it, so it can sign).
  ${FOREST_WALLET_PATH} --remote-wallet send --from "${genesis_addr}" "${test_addr}" "${DEVNET_TEST_FUND_AMT}"

  echo "Devnet wallet env initialised:"
  echo "  FULLNODE_API_INFO=<token>:/ip4/127.0.0.1/tcp/${FOREST_RPC_PORT}/http"
  echo "  FOREST_TEST_PRELOADED_ADDRESS=${FOREST_TEST_PRELOADED_ADDRESS}"
  echo "  Funding ${test_addr} with ${DEVNET_TEST_FUND_AMT} from ${genesis_addr}..."

  # Wait for the funding transfer to mine.
  local attempt balance
  for attempt in $(seq 1 120); do
    balance="$(${FOREST_WALLET_PATH} --remote-wallet balance "${test_addr}" --exact-balance)"
    if [[ "${balance}" != "0 FIL" ]]; then
      break
    fi
    sleep 5
  done
  if [[ "${balance}" == "0 FIL" ]]; then
    echo "ERROR: dedicated test wallet ${test_addr} was not funded in time" >&2
    return 1
  fi

  ${FOREST_CLI_PATH} chain head
  echo "  balance=${balance}"
}
