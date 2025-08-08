#!/usr/bin/env bash
# This script tests RPC API stateful tests on a live forest node.
# It requires both `forest`, `forest-wallet` and `forest-tool` to be in the PATH.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"


usage() {
  echo "Usage: $0 <PRELOADED_WALLET_STRING>"
  exit 1
}

if [ -z "$1" ]
  then
    usage
fi

echo "$1" > preloaded_wallet.key

forest_init "$@"

$FOREST_WALLET_PATH import preloaded_wallet.key
$FOREST_WALLET_PATH --remote-wallet import preloaded_wallet.key

TO_ADDRESS="t410fp6e7drelxau7nf76tcn6gva22t5jafefhevubwi"
FROM_ADDRESS="t410f2avianksmit2cl2bqk53qant7nm7rdmk63twa5y"
PAYLOAD="40c10f190000000000000000000000007f89f1c48bb829f697fe989be3541ad4fa9014850000000000000000000000000000000000000000000000000de0b6b3a7640000"
TOPIC="0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"

$FOREST_TOOL_PATH api test-stateful "$FULLNODE_API_INFO" \
  --to "$TO_ADDRESS" \
  --from "$FROM_ADDRESS" \
  --payload "$PAYLOAD" \
  --topic "$TOPIC"
