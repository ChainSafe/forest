#!/usr/bin/env bash

# This script is used to test the `forest-tool` commands that do not
# require a running `forest` node.

set -euxo pipefail

FOREST_TOOL_PATH="forest-tool"

# This should be a unit tests but it takes longer than 30 seconds.
"$FOREST_TOOL_PATH" state-migration actor-bundle
