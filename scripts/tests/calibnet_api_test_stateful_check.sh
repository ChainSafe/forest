#!/usr/bin/env bash
# This script tests RPC API stateful tests on a live forest node.
# It requires both `forest`, `forest-wallet` and `forest-tool` to be in the PATH.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

usage() {
  echo "Usage: $0 <CALIBNET_T4_WALLET_PRIVATE_KEY>"
  exit 1
}

if [ -z "$1" ]
  then
    usage
fi

echo "$1" > calibnet_t4_wallet.key

forest_init "$@"

$FOREST_WALLET_PATH --remote-wallet import calibnet_t4_wallet.key

# This is the address of a Calibnet pre-deployed simple ERC20 contract.
# You can find the hex and source code in 'src/tool/subcommands/api_cmd/contracts/erc20'.
TO_ADDRESS="t410fx6e5kwnpq7qohwmeru3kudpkmcrgifdwbytwioi"

FROM_ADDRESS="t410fnqf2kjzdyeuolsquhdpjjvftgwgs3q2t2zvjdja"

# This payload corresponds to minting new tokens for the 'FROM_ADDRESS' and will trigger a log event upon success.
# To compute the calldata, you can use the 'cast calldata' subcommand with the following arguments:
# cast calldata "mint(address,uint256)" 0xbf89d559af87e0e3d9848d36aa0dea60a2641476 1000000000000000000
#
# Note that 0xbf89d559af87e0e3d9848d36aa0dea60a2641476 is the Ethereum address corresponding to the contract f4 address.
PAYLOAD="0x40c10f19000000000000000000000000bf89d559af87e0e3d9848d36aa0dea60a26414760000000000000000000000000000000000000000000000000de0b6b3a7640000"

# This topic is derived using the keccak256 hash of the event signature 'Mint(address,uint256)'
# To compute the topic, you can use the 'cast keccak256' subcommand with the following argument:
# cast keccak256 "Mint(address,uint256)"
TOPIC="0x0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885"

$FOREST_TOOL_PATH api test-stateful \
  --to "$TO_ADDRESS" \
  --from "$FROM_ADDRESS" \
  --payload "$PAYLOAD" \
  --topic "$TOPIC"
