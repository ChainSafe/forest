#!/bin/bash
# Forest + Lotus past TARGET_HEIGHT (Lotus Miner mining via Forest).

set -eu

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

wait_forest_and_lotus_past_target "Lotus validated Forest-produced blocks" "$(check_timeout_secs)"

popd
