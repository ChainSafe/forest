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

"$FOREST_CLI_PATH" fetch-params --keys

: "cleaning an empty database doesn't fail (see #2811)"
"$FOREST_CLI_PATH" --chain calibnet db clean --force
"$FOREST_CLI_PATH" --chain calibnet db clean --force


: fetch snapshot
pushd "$(mktemp --directory)"
"$FOREST_CLI_PATH" --chain calibnet snapshot fetch --vendor forest
"$FOREST_CLI_PATH" --chain calibnet snapshot fetch --vendor filops
# this will fail if they happen to have the same height - we should change the format of our filenames
test "$(num-files-here)" -eq 2
rm -- *
popd



: validate calibnet snapshot
pushd "$(mktemp --directory)"
    : : fetch a calibnet snapshot
    "$FOREST_CLI_PATH" --chain calibnet snapshot fetch
    test "$(num-files-here)" -eq 1

    validate_me=$(find . -type f | head -1)
    : : validating under calibnet chain should succeed
    "$FOREST_CLI_PATH" --chain calibnet snapshot validate "$validate_me" --force

    : : validating under mainnet chain should fail
    if "$FOREST_CLI_PATH" --chain mainnet snapshot validate "$validate_me" --force; then
        exit 1
    fi
rm -- *
popd

