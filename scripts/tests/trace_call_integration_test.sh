#!/usr/bin/env bash
# Trace Call Comparison Test - Compares Forest's trace_call with Anvil's debug_traceCall
# Usage: ./trace_call_integration_test.sh [--deploy] [--verbose]
set -e

# --- Parse Flags ---
DEPLOY_CONTRACT=false
VERBOSE=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --deploy)  DEPLOY_CONTRACT=true; shift ;;
        --verbose) VERBOSE=true; shift ;;
        *) echo "Usage: $0 [--deploy] [--verbose]"; exit 1 ;;
    esac
done

# --- Configuration ---
FOREST_RPC_URL="${FOREST_RPC_URL:-http://localhost:2345/rpc/v1}"
ANVIL_RPC_URL="${ANVIL_RPC_URL:-http://localhost:8545}"
FOREST_ACCOUNT="${FOREST_ACCOUNT:- "0xb7aa1e9c847cda5f60f1ae6f65c3eae44848d41f"}"
FOREST_CONTRACT="${FOREST_CONTRACT:- "0x8724d2eb7f86ebaef34e050b02fac6c268e56775"}"
ANVIL_ACCOUNT="${ANVIL_ACCOUNT:-"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"}"
ANVIL_CONTRACT="${ANVIL_CONTRACT:-"0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"}"
ANVIL_PRIVATE_KEY="${ANVIL_PRIVATE_KEY:- ""}"

GREEN='\033[0;32m' RED='\033[0;31m' BLUE='\033[0;34m' YELLOW='\033[0;33m' NC='\033[0m'
PASS_COUNT=0 FAIL_COUNT=0

# --- Dependency Check ---
command -v jq &>/dev/null || { echo "Error: jq is required"; exit 1; }
command -v curl &>/dev/null || { echo "Error: curl is required"; exit 1; }

# --- Unified RPC Dispatcher ---
# Single entry point for all RPC calls - removes JSON-RPC boilerplate from test logic
call_rpc() {
    local url="$1" method="$2" params="$3"
    curl -s -X POST "$url" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}"
}

# --- RPC Health Check ---
check_rpc() {
    local name="$1" url="$2"
    local resp=$(call_rpc "$url" "eth_chainId" "[]")
    if [ -z "$resp" ] || echo "$resp" | jq -e '.error' &>/dev/null; then
        echo -e "${RED}Error: Cannot connect to $name at $url${NC}"
        return 1
    fi
    return 0
}

check_rpc "Forest" "$FOREST_RPC_URL" || exit 1
check_rpc "Anvil" "$ANVIL_RPC_URL" || exit 1

# --- Deploy Contract (if requested) ---
if [ "$DEPLOY_CONTRACT" = true ]; then
    command -v forge &>/dev/null || { echo "Error: forge is required for --deploy"; exit 1; }
    echo -e "${YELLOW}Deploying Tracer contract on Anvil...${NC}"
    CONTRACT_PATH="src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol"
    ANVIL_CONTRACT=$(forge create "$CONTRACT_PATH:Tracer" \
        --rpc-url "$ANVIL_RPC_URL" \
        --private-key "$ANVIL_PRIVATE_KEY" \
        --broadcast --json 2>/dev/null | jq -r '.deployedTo')
    echo -e "Deployed to: ${GREEN}$ANVIL_CONTRACT${NC}"
fi

# --- Normalization Helpers ---
# Convert different node outputs into a standard format for comparison

# Normalize empty values: null, "", "0x" -> "0x"
normalize_empty() {
    local val="$1"
    [[ "$val" == "null" || -z "$val" ]] && echo "0x" || echo "$val"
}

# Get balance change type from Forest's Parity Delta format
# Returns: "unchanged", "changed", "added", or "removed"
get_balance_type() {
    local val="$1"
    # Handle unchanged cases
    if [[ "$val" == "=" || "$val" == "\"=\"" || "$val" == "null" || -z "$val" ]]; then
        echo "unchanged"
        return
    fi
    # Check for Delta types
    if echo "$val" | jq -e 'has("*")' &>/dev/null; then
        echo "changed"
    elif echo "$val" | jq -e 'has("+")' &>/dev/null; then
        echo "added"
    elif echo "$val" | jq -e 'has("-")' &>/dev/null; then
        echo "removed"
    else
        echo "unchanged"
    fi
}

assert_eq() {
    local label="$1" f_val="$2" a_val="$3"
    
    # Normalize: lowercase and treat null/0x/empty as equivalent
    local f_norm=$(echo "$f_val" | tr '[:upper:]' '[:lower:]')
    local a_norm=$(echo "$a_val" | tr '[:upper:]' '[:lower:]')
    [[ "$f_norm" == "null" || -z "$f_norm" ]] && f_norm="0x"
    [[ "$a_norm" == "null" || -z "$a_norm" ]] && a_norm="0x"
    
    if [ "$f_norm" = "$a_norm" ]; then
        echo -e "  ${GREEN}[PASS]${NC} $label: $f_val"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} $label: (Forest: $f_val | Anvil: $a_val)"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
}

assert_both_have_error() {
    local f_err="$1" a_err="$2"
    if [[ -n "$f_err" && "$f_err" != "null" ]] && [[ -n "$a_err" && "$a_err" != "null" ]]; then
        echo -e "  ${GREEN}[PASS]${NC} Error: both have error"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Error: (Forest: '$f_err' | Anvil: '$a_err')"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
}

# Compares Forest's trace_call [trace] with Anvil's debug_traceCall (callTracer)
# Types: "standard" (default), "revert", "deep"
test_trace() {
    local name="$1" data="$2" type="${3:-standard}"
    echo -e "${BLUE}--- $name ---${NC}"

    # Forest: trace_call with trace
    local f_params="[{\"from\":\"$FOREST_ACCOUNT\",\"to\":\"$FOREST_CONTRACT\",\"data\":\"$data\"},[\"trace\"],\"latest\"]"
    local f_resp=$(call_rpc "$FOREST_RPC_URL" "trace_call" "$f_params")

    # Anvil: debug_traceCall with callTracer
    local a_params="[{\"from\":\"$ANVIL_ACCOUNT\",\"to\":\"$ANVIL_CONTRACT\",\"data\":\"$data\"},\"latest\",{\"tracer\":\"callTracer\"}]"
    local a_resp=$(call_rpc "$ANVIL_RPC_URL" "debug_traceCall" "$a_params")

    [[ "$VERBOSE" = true ]] && echo -e "${YELLOW}Forest:${NC} $f_resp\n${YELLOW}Anvil:${NC} $a_resp"

    # Extract & compare input (common to all types)
    local f_input=$(echo "$f_resp" | jq -r '.result.trace[0].action.input')
    local a_input=$(echo "$a_resp" | jq -r '.result.input')
    assert_eq "Input" "$f_input" "$a_input"

    # Type-specific comparisons
    case $type in
        revert)
            local f_err=$(echo "$f_resp" | jq -r '.result.trace[0].error // empty')
            local a_err=$(echo "$a_resp" | jq -r '.result.error // empty')
            assert_both_have_error "$f_err" "$a_err"
            ;;
        deep)
            local f_count=$(echo "$f_resp" | jq -r '.result.trace | length')
            local a_count=$(echo "$a_resp" | jq '[.. | objects | select(has("type"))] | length')
            assert_eq "TraceCount" "$f_count" "$a_count"
            ;;
        *)
            local f_out=$(normalize_empty "$(echo "$f_resp" | jq -r '.result.trace[0].result.output // .result.output')")
            local a_out=$(normalize_empty "$(echo "$a_resp" | jq -r '.result.output')")
            local f_sub=$(echo "$f_resp" | jq -r '.result.trace[0].subtraces // 0')
            local a_sub=$(echo "$a_resp" | jq -r '.result.calls // [] | length')
            assert_eq "Output" "$f_out" "$a_out"
            assert_eq "Subcalls" "$f_sub" "$a_sub"
            ;;
    esac
    echo ""
}

# Compares Forest's trace_call [stateDiff] with Anvil's prestateTracer (diffMode)
test_state_diff() {
    local name="$1" data="$2" value="${3:-0x0}" expect="${4:-unchanged}"
    echo -e "${BLUE}--- $name (stateDiff) ---${NC}"

    # Forest: trace_call with stateDiff
    local f_params="[{\"from\":\"$FOREST_ACCOUNT\",\"to\":\"$FOREST_CONTRACT\",\"data\":\"$data\",\"value\":\"$value\"},[\"stateDiff\"],\"latest\"]"
    local f_resp=$(call_rpc "$FOREST_RPC_URL" "trace_call" "$f_params")

    # Anvil: prestateTracer with diffMode
    local a_params="[{\"from\":\"$ANVIL_ACCOUNT\",\"to\":\"$ANVIL_CONTRACT\",\"data\":\"$data\",\"value\":\"$value\"},\"latest\",{\"tracer\":\"prestateTracer\",\"tracerConfig\":{\"diffMode\":true}}]"
    local a_resp=$(call_rpc "$ANVIL_RPC_URL" "debug_traceCall" "$a_params")

    [[ "$VERBOSE" = true ]] && echo -e "${YELLOW}Forest:${NC} $f_resp\n${YELLOW}Anvil:${NC} $a_resp"

    # Extract contract addresses (lowercase for jq lookup)
    local f_contract_lower=$(echo "$FOREST_CONTRACT" | tr '[:upper:]' '[:lower:]')
    local a_contract_lower=$(echo "$ANVIL_CONTRACT" | tr '[:upper:]' '[:lower:]')

    # Extract Forest stateDiff balance
    local f_diff=$(echo "$f_resp" | jq '.result.stateDiff // {}')
    local f_bal=$(echo "$f_diff" | jq -r --arg a "$f_contract_lower" '.[$a].balance // "="')
    local f_type=$(get_balance_type "$f_bal")

    # Extract Anvil pre/post balance and determine change type
    local a_pre_bal=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" '.result.pre[$a].balance // "0x0"')
    local a_post_bal=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" '.result.post[$a].balance // "0x0"')
    local a_type="unchanged"
    [[ "$a_pre_bal" != "$a_post_bal" ]] && a_type="changed"

    # Semantic assertions - compare intent, not raw values
    assert_eq "Forest matches Expected" "$f_type" "$expect"
    assert_eq "Forest matches Anvil" "$f_type" "$a_type"
    echo ""
}

echo "=============================================="
echo "Trace Call Comparison: Forest vs Anvil"
echo "=============================================="
echo "Forest: $FOREST_RPC_URL | Contract: $FOREST_CONTRACT"
echo "Anvil:  $ANVIL_RPC_URL | Contract: $ANVIL_CONTRACT"
echo ""

# --- Trace Tests ---
echo -e "${BLUE}=== Trace Tests ===${NC}"
echo ""

test_trace "setX(123)" \
    "0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b"

test_trace "doRevert()" \
    "0xafc874d2" \
    "revert"

test_trace "callSelf(999)" \
    "0xa1a8859500000000000000000000000000000000000000000000000000000000000003e7"

test_trace "complexTrace()" \
    "0x6659ab96"

test_trace "deepTrace(3)" \
    "0x0f3a17b80000000000000000000000000000000000000000000000000000000000000003" \
    "deep"

# --- StateDiff Tests ---
echo -e "${BLUE}=== StateDiff Tests ===${NC}"
echo ""

test_state_diff "deposit() with 1 ETH" \
    "0xd0e30db0" \
    "0xde0b6b3a7640000" \
    "changed"

test_state_diff "setX(42) no value" \
    "0x4018d9aa000000000000000000000000000000000000000000000000000000000000002a" \
    "0x0" \
    "unchanged"

# --- Results ---
echo "=============================================="
echo -e "Results: ${GREEN}Passed: $PASS_COUNT${NC} | ${RED}Failed: $FAIL_COUNT${NC}"
[[ $FAIL_COUNT -gt 0 ]] && exit 1 || exit 0
