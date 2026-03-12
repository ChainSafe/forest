#!/usr/bin/env bash
# Compare Forest `trace_call` (Parity-style traces and stateDiff) against Anvil
# `debug_traceCall` (Geth-style prestateTracer) using the Tracer.sol contract.
# Verifies call traces, balance diffs, and EVM storage diffs stay in sync.

set -euo pipefail

DEPLOY_CONTRACT=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --deploy) DEPLOY_CONTRACT=true; shift ;;
        *) echo "Usage: $0 [--deploy]"; exit 1 ;;
    esac
done

# --- Configuration ---
FOREST_RPC_URL="${FOREST_RPC_URL:-http://localhost:2345/rpc/v1}"
ANVIL_RPC_URL="${ANVIL_RPC_URL:-http://localhost:8545}"
FOREST_ACCOUNT="${FOREST_ACCOUNT:-0xb7aa1e9c847cda5f60f1ae6f65c3eae44848d41f}"
FOREST_CONTRACT="${FOREST_CONTRACT:-0x73a43475aa2ccb14246613708b399f4b2ba546c7}"
ANVIL_ACCOUNT="${ANVIL_ACCOUNT:-0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266}"
ANVIL_CONTRACT="${ANVIL_CONTRACT:-0x5FbDB2315678afecb367f032d93F642f64180aa3}"
# -- This private key is of anvil dev node --
ANVIL_PRIVATE_KEY="${ANVIL_PRIVATE_KEY:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}"

GREEN='\033[0;32m' RED='\033[0;31m' BLUE='\033[0;34m' YELLOW='\033[0;33m' NC='\033[0m'
PASS_COUNT=0 FAIL_COUNT=0

command -v jq &>/dev/null || { echo "Error: jq is required"; exit 1; }
command -v curl &>/dev/null || { echo "Error: curl is required"; exit 1; }

# Call JSON-RPC method using jq for body construction (type-safe, no escaping needed)
call_rpc() {
    local url="$1" method="$2" params="$3"
    local body=$(jq -n --arg method "$method" --argjson params "$params" \
        '{"jsonrpc": "2.0", "id": 1, "method": $method, "params": $params}')
    curl -s -X POST "$url" -H "Content-Type: application/json" -d "$body"
}

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

if [ "$DEPLOY_CONTRACT" = true ]; then
    command -v forge &>/dev/null || { echo "Error: forge is required for --deploy"; exit 1; }
    echo -e "${YELLOW}Deploying Tracer contract on Anvil...${NC}"
    CONTRACT_PATH="src/tool/subcommands/api_cmd/contracts/tracer/Tracer.sol"
    ANVIL_CONTRACT=$(forge create "$CONTRACT_PATH:Tracer" \
        --rpc-url "$ANVIL_RPC_URL" \
        --private-key "$ANVIL_PRIVATE_KEY" \
        --broadcast --json 2>/dev/null | jq -r '.deployedTo')
    if [[ -z "$ANVIL_CONTRACT" || "$ANVIL_CONTRACT" == "null" ]]; then
        echo -e "${RED}Error: Contract deployment failed${NC}"
        exit 1
    fi
    echo -e "Deployed to: ${GREEN}$ANVIL_CONTRACT${NC}"
fi

normalize_empty() {
    local val="$1"
    [[ "$val" == "null" || -z "$val" ]] && echo "0x" || echo "$val"
}

get_delta_type() {
    local val="$1"
    if [[ "$val" == "=" || "$val" == "\"=\"" || "$val" == "null" || -z "$val" ]]; then
        echo "unchanged"
    elif echo "$val" | jq -e 'has("*")' &>/dev/null; then
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
        echo -e "  ${GREEN}[PASS]${NC} Both have error"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Error mismatch (Forest: '$f_err' | Anvil: '$a_err')"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
}

test_trace() {
    local name="$1" data="$2" type="${3:-standard}"
    echo -e "${BLUE}--- $name ---${NC}"

    local f_params=$(jq -n \
        --arg from "$FOREST_ACCOUNT" \
        --arg to "$FOREST_CONTRACT" \
        --arg data "$data" \
        '[{"from": $from, "to": $to, "data": $data}, ["trace"], "latest"]')
    local f_resp=$(call_rpc "$FOREST_RPC_URL" "trace_call" "$f_params")

    local a_params=$(jq -n \
        --arg from "$ANVIL_ACCOUNT" \
        --arg to "$ANVIL_CONTRACT" \
        --arg data "$data" \
        '[{"from": $from, "to": $to, "data": $data}, "latest", {"tracer": "callTracer"}]')
    local a_resp=$(call_rpc "$ANVIL_RPC_URL" "debug_traceCall" "$a_params")

    local f_input=$(echo "$f_resp" | jq -r '.result.trace[0].action.input')
    local a_input=$(echo "$a_resp" | jq -r '.result.input')
    assert_eq "Input" "$f_input" "$a_input"

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

test_balance_diff() {
    local name="$1" data="$2" value="${3:-0x0}" expect="${4:-unchanged}"
    echo -e "${BLUE}--- $name (balance) ---${NC}"

    local f_params=$(jq -n \
        --arg from "$FOREST_ACCOUNT" \
        --arg to "$FOREST_CONTRACT" \
        --arg data "$data" \
        --arg value "$value" \
        '[{"from": $from, "to": $to, "data": $data, "value": $value}, ["stateDiff"], "latest"]')
    local f_resp=$(call_rpc "$FOREST_RPC_URL" "trace_call" "$f_params")

    local a_params=$(jq -n \
        --arg from "$ANVIL_ACCOUNT" \
        --arg to "$ANVIL_CONTRACT" \
        --arg data "$data" \
        --arg value "$value" \
        '[{"from": $from, "to": $to, "data": $data, "value": $value}, "latest", {"tracer": "prestateTracer", "tracerConfig": {"diffMode": true}}]')
    local a_resp=$(call_rpc "$ANVIL_RPC_URL" "debug_traceCall" "$a_params")

    local f_contract_lower=$(echo "$FOREST_CONTRACT" | tr '[:upper:]' '[:lower:]')
    local a_contract_lower=$(echo "$ANVIL_CONTRACT" | tr '[:upper:]' '[:lower:]')

    local f_bal=$(echo "$f_resp" | jq -r --arg a "$f_contract_lower" '.result.stateDiff[$a].balance // "="')
    local f_type=$(get_delta_type "$f_bal")

    local a_pre_bal=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" '.result.pre[$a].balance // "0x0"')
    local a_post_bal=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" '.result.post[$a].balance // "0x0"')
    local a_type="unchanged"
    [[ "$a_pre_bal" != "$a_post_bal" ]] && a_type="changed"

    assert_eq "BalanceChange" "$f_type" "$expect"
    assert_eq "ForestMatchesAnvil" "$f_type" "$a_type"

    # Compare actual balance values when changed
    if [ "$f_type" = "changed" ]; then
        local f_bal_to=$(echo "$f_bal" | jq -r '.["*"].to // empty')
        [[ -n "$f_bal_to" && -n "$a_post_bal" && "$a_post_bal" != "0x0" ]] && \
            assert_eq "BalanceTo" "$f_bal_to" "$a_post_bal"
    fi
    echo ""
}

test_storage_diff() {
    local name="$1" data="$2" slot="$3" expect_type="${4:-changed}"
    echo -e "${BLUE}--- $name (storage) ---${NC}"

    local f_params=$(jq -n \
        --arg from "$FOREST_ACCOUNT" \
        --arg to "$FOREST_CONTRACT" \
        --arg data "$data" \
        '[{"from": $from, "to": $to, "data": $data}, ["stateDiff"], "latest"]')
    local f_resp=$(call_rpc "$FOREST_RPC_URL" "trace_call" "$f_params")

    local a_params=$(jq -n \
        --arg from "$ANVIL_ACCOUNT" \
        --arg to "$ANVIL_CONTRACT" \
        --arg data "$data" \
        '[{"from": $from, "to": $to, "data": $data}, "latest", {"tracer": "prestateTracer", "tracerConfig": {"diffMode": true}}]')
    local a_resp=$(call_rpc "$ANVIL_RPC_URL" "debug_traceCall" "$a_params")

    local f_contract_lower=$(echo "$FOREST_CONTRACT" | tr '[:upper:]' '[:lower:]')
    local a_contract_lower=$(echo "$ANVIL_CONTRACT" | tr '[:upper:]' '[:lower:]')

    # Forest: Extract storage slot delta and values
    local f_slot_data=$(echo "$f_resp" | jq -r --arg a "$f_contract_lower" --arg s "$slot" '.result.stateDiff[$a].storage[$s] // null')
    local f_type=$(get_delta_type "$f_slot_data")

    # Anvil: Determine if the storage slot changed.
    # Per reth/Parity behavior, all storage slot transitions on existing accounts
    # are Delta::Changed ("*"), including zero→nonzero and nonzero→zero.
    local a_pre_has=$(echo "$a_resp" | jq --arg a "$a_contract_lower" --arg s "$slot" '.result.pre[$a].storage[$s] != null')
    local a_post_has=$(echo "$a_resp" | jq --arg a "$a_contract_lower" --arg s "$slot" '.result.post[$a].storage[$s] != null')
    local a_type="unchanged"
    if [[ "$a_pre_has" == "true" || "$a_post_has" == "true" ]]; then
        a_type="changed"
    fi

    assert_eq "StorageChangeType" "$f_type" "$expect_type"
    assert_eq "ForestMatchesAnvil" "$f_type" "$a_type"

    # Compare values: Forest uses "*": { "from": ..., "to": ... } for all changes.
    # Anvil may omit the slot from pre (zero→nonzero) or post (nonzero→zero).
    if [ "$f_type" = "changed" ]; then
        local f_from=$(echo "$f_slot_data" | jq -r '.["*"].from // empty')
        local f_to=$(echo "$f_slot_data" | jq -r '.["*"].to // empty')

        # Anvil pre value: present means nonzero existed, absent means was zero
        local a_from=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" --arg s "$slot" '.result.pre[$a].storage[$s] // empty')
        # Anvil post value: present means nonzero now, absent means cleared to zero
        local a_to=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" --arg s "$slot" '.result.post[$a].storage[$s] // empty')

        if [[ -n "$f_to" && -n "$a_to" ]]; then
            assert_eq "StorageTo" "$f_to" "$a_to"
        elif [[ -n "$f_from" && -n "$a_from" ]]; then
            assert_eq "StorageFrom" "$f_from" "$a_from"
        fi
    fi
    echo ""
}

test_storage_multiple() {
    local name="$1" data="$2" expect_type="$3"
    shift 3
    local slots=("$@")
    echo -e "${BLUE}--- $name (multi-storage) ---${NC}"

    local f_params=$(jq -n \
        --arg from "$FOREST_ACCOUNT" \
        --arg to "$FOREST_CONTRACT" \
        --arg data "$data" \
        '[{"from": $from, "to": $to, "data": $data}, ["stateDiff"], "latest"]')
    local f_resp=$(call_rpc "$FOREST_RPC_URL" "trace_call" "$f_params")

    local a_params=$(jq -n \
        --arg from "$ANVIL_ACCOUNT" \
        --arg to "$ANVIL_CONTRACT" \
        --arg data "$data" \
        '[{"from": $from, "to": $to, "data": $data}, "latest", {"tracer": "prestateTracer", "tracerConfig": {"diffMode": true}}]')
    local a_resp=$(call_rpc "$ANVIL_RPC_URL" "debug_traceCall" "$a_params")

    local f_contract_lower=$(echo "$FOREST_CONTRACT" | tr '[:upper:]' '[:lower:]')
    local a_contract_lower=$(echo "$ANVIL_CONTRACT" | tr '[:upper:]' '[:lower:]')

    local f_slot_count=$(echo "$f_resp" | jq -r --arg a "$f_contract_lower" '.result.stateDiff[$a].storage | length')
    local a_slot_count=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" '.result.post[$a].storage | length')
    assert_eq "SlotCount" "$f_slot_count" "$a_slot_count"

    for slot in "${slots[@]}"; do
        local f_slot_data=$(echo "$f_resp" | jq -r --arg a "$f_contract_lower" --arg s "$slot" '.result.stateDiff[$a].storage[$s] // null')
        local f_type=$(get_delta_type "$f_slot_data")
        local f_to_val=$(echo "$f_slot_data" | jq -r '.["*"].to // empty')
        local a_post_val=$(echo "$a_resp" | jq -r --arg a "$a_contract_lower" --arg s "$slot" '.result.post[$a].storage[$s] // empty')

        local slot_short="${slot: -4}"
        assert_eq "Slot${slot_short}Type" "$f_type" "$expect_type"
        assert_eq "Slot${slot_short}Value" "$f_to_val" "$a_post_val"
    done
    echo ""
}

# =============================================================================
# Main Execution
# =============================================================================
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

# delegateSelf(999) - selector: 0x8f5e07b8
test_trace "delegateSelf(999)" \
    "0x8f5e07b800000000000000000000000000000000000000000000000000000000000003e7"

# wideTrace(3, 1) - selector: 0x56d15f7c, 3 siblings each 1 deep
test_trace "wideTrace(3,1)" \
    "0x56d15f7c00000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000001" \
    "deep"

# failAtDepth(3, 2) - revert at depth 2 inside depth-3 recursion
# selector: 0x68bcf9e2
test_trace "failAtDepth(3,2)" \
    "0x68bcf9e200000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000002" \
    "revert"

# --- Balance Diff Tests ---
echo -e "${BLUE}=== Balance Diff Tests ===${NC}"
echo ""

test_balance_diff "deposit() with 1 ETH" \
    "0xd0e30db0" \
    "0xde0b6b3a7640000" \
    "changed"

test_balance_diff "setX(42) no value" \
    "0x4018d9aa000000000000000000000000000000000000000000000000000000000000002a" \
    "0x0" \
    "unchanged"

# --- Storage Diff Tests ---
echo -e "${BLUE}=== Storage Diff Tests ===${NC}"
echo ""

# Note: Each trace_call is stateless — simulated against on-chain state.
# Tests do NOT affect each other. Slot 0 (x) is always 42 on-chain.

# Slot 0: setX(123) modifies existing value (42 → 123 = changed)
test_storage_diff "setX(123) - change slot 0" \
    "0x4018d9aa000000000000000000000000000000000000000000000000000000000000007b" \
    "0x0000000000000000000000000000000000000000000000000000000000000000" \
    "changed"

# Slot 0: setX(42) writes same value as on-chain (42 → 42 = unchanged, no storage entry)
test_storage_diff "setX(42) - no-op same value" \
    "0x4018d9aa000000000000000000000000000000000000000000000000000000000000002a" \
    "0x0000000000000000000000000000000000000000000000000000000000000000" \
    "unchanged"

# Slot 0: setX(0) clears existing value (42 → 0 = changed per reth/Parity)
test_storage_diff "setX(0) - clear slot 0" \
    "0x4018d9aa0000000000000000000000000000000000000000000000000000000000000000" \
    "0x0000000000000000000000000000000000000000000000000000000000000000" \
    "changed"

# Slot 2: storageTestA (starts empty on-chain, 0 → 100 = changed per reth/Parity)
test_storage_diff "storageAdd(100) - write slot 2" \
    "0x55cb64b40000000000000000000000000000000000000000000000000000000000000064" \
    "0x0000000000000000000000000000000000000000000000000000000000000002" \
    "changed"

# Multiple slots: storageTestA(10), storageTestB(20), storageTestC(30) - all start empty
test_storage_multiple "storageMultiple(10,20,30) - slots 2,3,4" \
    "0x310af204000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000014000000000000000000000000000000000000000000000000000000000000001e" \
    "changed" \
    "0x0000000000000000000000000000000000000000000000000000000000000002" \
    "0x0000000000000000000000000000000000000000000000000000000000000003" \
    "0x0000000000000000000000000000000000000000000000000000000000000004"

# --- Results ---
echo "=============================================="
echo -e "Results: ${GREEN}Passed: $PASS_COUNT${NC} | ${RED}Failed: $FAIL_COUNT${NC}"
[[ $FAIL_COUNT -gt 0 ]] && exit 1 || exit 0
