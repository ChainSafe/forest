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

snapshot_dir=$(mktemp --directory)

# return the first snapshot path listed by forest
function pop-snapshot-path() {
    "$FOREST_CLI_PATH" snapshot list --snapshot-dir "$snapshot_dir" \
        | grep path \
        | sed 's/^\tpath: //' \
        | head --lines=1
}

function clean-snapshot-dir() {
    rm --recursive --force --verbose -- "$snapshot_dir"
    mkdir --parents -- "$snapshot_dir"
}

"$FOREST_CLI_PATH" fetch-params --keys

: fetch snapshot
"$FOREST_CLI_PATH" --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir" --vendor forest
"$FOREST_CLI_PATH" --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir" --vendor filops
clean-snapshot-dir


: "cleaning an empty database doesn't fail (see #2811)"
"$FOREST_CLI_PATH" --chain calibnet db clean --force
"$FOREST_CLI_PATH" --chain calibnet db clean --force

: validate calibnet snapshot
    clean-snapshot-dir

    : : fetch a calibnet snapshot
    "$FOREST_CLI_PATH" --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir"
    validate_me=$(pop-snapshot-path)

    : : validating under calibnet chain should succeed
    "$FOREST_CLI_PATH" --chain calibnet snapshot validate "$validate_me" --force

    : : validating under mainnet chain should fail
    if "$FOREST_CLI_PATH" --chain mainnet snapshot validate "$validate_me" --force; then
        exit 1
    fi

    clean-snapshot-dir
