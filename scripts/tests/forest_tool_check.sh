#!/usr/bin/env bash

# This script is used to test the `forest-tool` commands that do not
# require a running `forest` node.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

"$FOREST_TOOL_PATH" state-migration actor-bundle
