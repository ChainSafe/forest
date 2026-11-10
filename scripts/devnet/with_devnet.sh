#!/bin/bash
# Bring up the devnet in $1, run the remaining arguments against it, and tear it down on exit.
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: with_devnet.sh <devnet-dir> <command...>" >&2
  exit 1
fi
DEVNET_DIR="$1"
shift
export DEVNET_DIR

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "${REPO_ROOT}"

# Volumes stay: `setup.sh` removes the data volumes on the next run and keeps the proof params.
trap 'docker compose --project-directory "${REPO_ROOT}/${DEVNET_DIR}" -f "${REPO_ROOT}/${DEVNET_DIR}/docker-compose.yml" down --remove-orphans || true' EXIT

pushd "${DEVNET_DIR}" >/dev/null
./setup.sh
./check.sh
popd >/dev/null

source ./scripts/devnet/test_harness.sh
devnet_test_env_init
"$@"
