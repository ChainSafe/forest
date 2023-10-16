#!/bin/bash
set -euxo pipefail

# This script tests the stateless mode of a forest node

source "$(dirname "$0")/harness.sh"

forest_init_stateless

echo "Verifying the heaviest tipset to be the genesis"
MSG=$($FOREST_CLI_PATH chain head)
assert_eq "$MSG" $'[\n  "bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4"\n]'
