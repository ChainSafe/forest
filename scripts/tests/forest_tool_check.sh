#!/usr/bin/env bash

# This script is used to test the `forest-tool` commands that do not
# require a running `forest` node.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

"$FOREST_TOOL_PATH" state-migration actor-bundle

# Exporting with an empty car should fail but not panic
touch empty.car
ERROR=$($FOREST_CLI_PATH archive export empty.car 2>&1 || true)
assert_eq "$ERROR" "Error: input not recognized as any kind of CAR data (.car, .car.zst, .forest.car)"
rm empty.car
