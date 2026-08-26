#!/bin/bash
# Bring up the devnet in $1, run the integration suites against it over Forest's
# RPC, and tear it down on exit. Shared by both the lotus- and forest-produced devnets.
set -euo pipefail

DEVNET_DIR="${1:?usage: run_integration_tests.sh <devnet-dir>}"
export DEVNET_DIR

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "${REPO_ROOT}"

trap 'docker compose --project-directory "${REPO_ROOT}/${DEVNET_DIR}" -f "${REPO_ROOT}/${DEVNET_DIR}/docker-compose.yml" down --remove-orphans --volumes || true' EXIT

pushd "${DEVNET_DIR}" >/dev/null
./setup.sh
./check.sh
popd >/dev/null

source ./scripts/devnet/test_harness.sh
devnet_test_env_init
forest-dev tests mpool
forest-dev tests wallet
forest-dev devnet eth-gas
