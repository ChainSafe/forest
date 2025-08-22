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

# This is the address of a Calibnet pre-deployed simple ERC20 contract.
# You can find the hex and source code in 'src/tool/subcommands/api_cmd/contracts/erc20'.
TO_ADDRESS="t410fp6e7drelxau7nf76tcn6gva22t5jafefhevubwi"

FROM_ADDRESS="t410f2avianksmit2cl2bqk53qant7nm7rdmk63twa5y"

# This payload corresponds to minting new tokens for the 'FROM_ADDRESS' and will trigger a log event upon success.
# To compute the calldata, you can use the 'cast calldata' subcommand with the following arguments:
# cast calldata "mint(address,uint256)" 0x7f89f1c48bb829f697fe989be3541ad4fa901485 1000000000000000000
#
# Note that 0x7f89f1c48bb829f697fe989be3541ad4fa901485 is the Ethereum address corresponding to the contract f4 address.
PAYLOAD="40c10f190000000000000000000000007f89f1c48bb829f697fe989be3541ad4fa9014850000000000000000000000000000000000000000000000000de0b6b3a7640000"

# This topic is derived using the keccak256 hash of the event signature 'Mint(address,uint256)'
# To compute the topic, you can use the 'cast keccak256' subcommand with the following argument:
# cast keccak256 "Mint(address,uint256)"
TOPIC="0x0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885"

$FOREST_TOOL_PATH api test-stateful "$FULLNODE_API_INFO" \
  --to "$TO_ADDRESS" \
  --from "$FROM_ADDRESS" \
  --payload "$PAYLOAD" \
  --topic "$TOPIC"
