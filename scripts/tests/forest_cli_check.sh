#!/usr/bin/env bash
# This script is used to test the `forest-cli` commands that do not
# require a running `forest` node.
#
# It's run in CI, and tests things that currently aren't tested (and don't make
# sense to test) in our rust test harness.
# This means things like fetching and validating snapshots from the web.
#
# It depends on the `forest-cli` binary being in the PATH.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

# Print the number of files in the present working directory
function num-files-here() {
    # list one file per line
    find . -type f \
        | wc --lines
}

"$FOREST_TOOL_PATH" fetch-params --keys

: "destroying an empty database doesn't fail (see #2811)"
"$FOREST_TOOL_PATH" db destroy --chain calibnet --force
"$FOREST_TOOL_PATH" db destroy --chain calibnet --force

: validate latest calibnet snapshot
pushd "$(mktemp --directory)"
    : : fetch a compressed calibnet snapshot
    "$FOREST_TOOL_PATH" snapshot fetch --chain calibnet
    test "$(num-files-here)" -eq 1
    uncompress_me=$(find . -type f | head -1)

    : : decompress it, as validate does not support compressed snapshots
    zstd --decompress --rm "$uncompress_me"

    validate_me=$(find . -type f | head -1)
    : : validating under calibnet chain should succeed
    "$FOREST_TOOL_PATH" snapshot validate --check-network calibnet "$validate_me"

    : : validating under mainnet chain should fail
    if "$FOREST_TOOL_PATH" snapshot validate --check-network mainnet "$validate_me"; then
        exit 1
    fi

    : : check that it contains at least one expected checkpoint
    # If calibnet is reset or the checkpoint interval is changed, this check has to be updated
    "$FOREST_TOOL_PATH" archive checkpoints "$validate_me" | grep bafy2bzaceatx7tlwdhez6vyias5qlhaxa54vjftigbuqzfsmdqduc6jdiclzc
rm -- *
popd

