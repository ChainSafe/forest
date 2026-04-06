#!/usr/bin/env bash
# This script tests forest-cli mpool nonce-fix and forest-cli mpool replace commands.
# It requires both `forest` and `forest-cli` to be in the PATH, plus a funded wallet.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

forest_wallet_init "$@"

ADDR=$($FOREST_WALLET_PATH list | tail -1 | cut -d ' ' -f1)

sleep 5s

# nonce-fix: CLI argument validation
: "nonce-fix: missing --addr should fail"
if $FOREST_CLI_PATH mpool nonce-fix 2>&1; then
  echo "FAIL: expected error without --addr"
  exit 1
fi

: "nonce-fix: manual mode missing --start should fail"
if $FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --end 10 2>&1; then
  echo "FAIL: expected error without --start"
  exit 1
fi

: "nonce-fix: manual mode missing --end should fail"
if $FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --start 5 2>&1; then
  echo "FAIL: expected error without --end"
  exit 1
fi

: "nonce-fix: end equals start should fail"
if $FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --start 5 --end 5 2>&1; then
  echo "FAIL: expected error with --end == --start"
  exit 1
fi

: "nonce-fix: end less than start should fail"
if $FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --start 5 --end 3 2>&1; then
  echo "FAIL: expected error with --end < --start"
  exit 1
fi

: "nonce-fix: invalid gas-fee-cap should fail"
if $FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --start 0 --end 1 --gas-fee-cap "not-a-number" 2>&1; then
  echo "FAIL: expected error with invalid --gas-fee-cap"
  exit 1
fi

# nonce-fix: auto mode -- no gap
: "nonce-fix: auto mode with no nonce gap"
OUTPUT=$($FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --auto)
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "No nonce gap found"

# nonce-fix: auto mode -- with gap
: "nonce-fix: create a nonce gap then auto-fill it"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
echo "Current nonce before gap: $NONCE"

$FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --start "$((NONCE + 2))" --end "$((NONCE + 3))"
sleep 2s

OUTPUT=$($FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --auto)
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "Creating 2 filler messages"

# nonce-fix: manual mode -- happy path
: "nonce-fix: manual mode creates filler messages"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
END=$((NONCE + 2))
OUTPUT=$($FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --start "$NONCE" --end "$END")
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "Creating 2 filler messages"

# nonce-fix: manual mode -- custom gas-fee-cap
: "nonce-fix: manual mode with custom gas-fee-cap"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
END=$((NONCE + 1))
OUTPUT=$($FOREST_CLI_PATH mpool nonce-fix --addr "$ADDR" --start "$NONCE" --end "$END" --gas-fee-cap "1000000000")
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "Creating 1 filler messages"

# nonce-fix: verify pending messages
: "nonce-fix: verify filler messages appear in pending"
PENDING=$($FOREST_CLI_PATH mpool pending --from "$ADDR" --cids)
echo "$PENDING"
if [ -z "$PENDING" ]; then
  echo "FAIL: expected pending messages from $ADDR"
  exit 1
fi

# replace: CLI argument validation
: "replace: missing required args should fail"
if $FOREST_CLI_PATH mpool replace 2>&1; then
  echo "FAIL: expected error without --from or --cid"
  exit 1
fi

: "replace: conflicting --cid and --from should fail"
if $FOREST_CLI_PATH mpool replace --cid bafy2bzaceaxm23epjsmh75yvzcecsrbavlmkcxnvuzock6waew7l7piyn2bkji --from "$ADDR" 2>&1; then
  echo "FAIL: expected error with conflicting --cid and --from"
  exit 1
fi

: "replace: --from without --nonce should fail"
if $FOREST_CLI_PATH mpool replace --from "$ADDR" 2>&1; then
  echo "FAIL: expected error without --nonce"
  exit 1
fi

: "replace: non-existent pending message should fail"
if $FOREST_CLI_PATH mpool replace --from "$ADDR" --nonce 999999999 --auto 2>&1; then
  echo "FAIL: expected error for non-existent pending message"
  exit 1
fi

: "replace: invalid --max-fee should fail"
if $FOREST_CLI_PATH mpool replace --from "$ADDR" --nonce 0 --auto --max-fee "abc" 2>&1; then
  echo "FAIL: expected error with invalid --max-fee"
  exit 1
fi

# replace: happy path -- send messages then replace immediately
TARGET=$($FOREST_WALLET_PATH new)
$FOREST_WALLET_PATH set-default "$ADDR"

: "replace: auto mode by --from and --nonce"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
$FOREST_WALLET_PATH send "$TARGET" "100 atto FIL"
OUTPUT=$($FOREST_CLI_PATH mpool replace --from "$ADDR" --nonce "$NONCE" --auto)
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "new message cid:"

: "replace: manual mode with gas params"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
$FOREST_WALLET_PATH send "$TARGET" "100 atto FIL"
OUTPUT=$($FOREST_CLI_PATH mpool replace --from "$ADDR" --nonce "$NONCE" \
  --gas-premium "100000000000" --gas-feecap "1000000000000" --gas-limit 2000000)
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "new message cid:"

: "replace: auto mode by --cid"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
MSG_CID=$($FOREST_WALLET_PATH send "$TARGET" "100 atto FIL")
echo "Message CID for replace: $MSG_CID"
OUTPUT=$($FOREST_CLI_PATH mpool replace --cid "$MSG_CID" --auto)
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "new message cid:"

: "replace: auto mode with --max-fee"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
$FOREST_WALLET_PATH send "$TARGET" "100 atto FIL"
OUTPUT=$($FOREST_CLI_PATH mpool replace --from "$ADDR" --nonce "$NONCE" --auto --max-fee "10000000000000")
echo "$OUTPUT"
echo "$OUTPUT" | grep -q "new message cid:"

: "replace: manual gas premium below RBF minimum should fail"
NONCE=$($FOREST_CLI_PATH mpool nonce "$ADDR")
$FOREST_WALLET_PATH send "$TARGET" "100 atto FIL"
if $FOREST_CLI_PATH mpool replace --from "$ADDR" --nonce "$NONCE" --gas-premium "1" 2>&1; then
  echo "FAIL: expected error with gas premium below RBF minimum"
  exit 1
fi

: "All mpool nonce-fix and replace tests passed"
