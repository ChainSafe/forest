#!/usr/bin/env bash

# Load .env
source .env || { echo "Failed to load .env"; exit 1; }

# Validate script arguments
[[ -z "$ADDRESS" || -z "$TRACER" || -z "$SEPOLIA_RPC_URL" ]] && {
  echo "ERROR: Set ADDRESS, TRACER, SEPOLIA_RPC_URL in .env"
  exit 1
}

echo "Generating trace_call test suite..."
echo "Tracer: $TRACER"
echo "Caller: $ADDRESS"

BALANCE=$(cast balance "$ADDRESS" --rpc-url "$SEPOLIA_RPC_URL")
echo "Caller balance: $BALANCE wei"
echo

# The array of test cases
declare -a TESTS=(
  # id:function_name:args:value_hex
  "1:setX(uint256):999:"
  "2:deposit():"
  "3:transfer(address,uint256):0x1111111111111111111111111111111111111111 500:"
  "4:callSelf(uint256):999:"
  "5:delegateSelf(uint256):777:"
  "6:staticRead():"
  "7:createChild():"
  "8:destroyAndSend():"
  "9:keccakIt(bytes32):0x000000000000000000000000000000000000000000000000000000000000abcd:"
  "10:doRevert():"
)

# 0x13880 is 80,000

# Remember: trace_call is not a real transaction
#
# It’s a simulation!
# RPC nodes limit gas to prevent:
#  - Infinite loops
#  - DoS attacks
#  - Memory exhaustion

# We generated reference results using Alchemy provider, so you will likely see params.gas != action.gas
# in the first trace

# Generate each test reference
for TEST in "${TESTS[@]}"; do
  IFS=':' read -r ID FUNC ARGS VALUE_HEX <<< "$TEST"

  echo "test$ID: $FUNC"

  # Encode calldata
  if [[ -z "$ARGS" ]]; then
    CALLDATA=$(cast calldata "$FUNC")
  else
    CALLDATA=$(cast calldata "$FUNC" $ARGS)
  fi

  # Build payload
  if [[ -n "$VALUE_HEX" ]]; then
    PAYLOAD=$(jq -n \
      --arg from "$ADDRESS" \
      --arg to "$TRACER" \
      --arg data "$CALLDATA" \
      --arghex value "$VALUE_HEX" \
      '{
         jsonrpc: "2.0",
         id: ($id | tonumber),
         method: "trace_call",
         params: [
           { from: $from, to: $to, data: $data, value: $value, gas: "0x13880" },
           ["trace"],
           "latest"
         ]
       }' --arg id "$ID")
  else
    PAYLOAD=$(jq -n \
      --arg from "$ADDRESS" \
      --arg to "$TRACER" \
      --arg data "$CALLDATA" \
      '{
         jsonrpc: "2.0",
         id: ($id | tonumber),
         method: "trace_call",
         params: [
           { from: $from, to: $to, data: $data, gas: "0x13880" },
           ["trace"],
           "latest"
         ]
       }' --arg id "$ID")
  fi

  # Send request
  RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    --data "$PAYLOAD" \
    "$SEPOLIA_RPC_URL")

  # Combine request + response
  JSON_TEST=$(jq -n \
    --argjson request "$(echo "$PAYLOAD" | jq '.')" \
    --argjson response "$(echo "$RESPONSE" | jq '.')" \
    '{ request: $request, response: $response }')

  # Save reference file
  FILENAME="./refs/test${ID}.json"
  echo "$JSON_TEST" | jq . > "$FILENAME"
  echo "Saved to $FILENAME"

  echo
done

echo "All test references have been generated."
