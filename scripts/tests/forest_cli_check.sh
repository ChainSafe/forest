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
    forest-cli snapshot list --snapshot-dir "$snapshot_dir" \
        | grep path \
        | sed 's/^\tpath: //' \
        | head --lines=1
}

function clean-snapshot-dir() {
    rm --recursive --force --verbose -- "$snapshot_dir"
    mkdir --parents -- "$snapshot_dir"
}

# TODO(aatifsyed): I don't really understand what this does
"$FOREST_CLI_PATH" fetch-params --keys

: fetch snapshot
"$FOREST_CLI_PATH" --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir"
clean-snapshot-dir


: "clean database (twice)"
# TODO(aatifsyed): I don't really understand what this tests
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

: intern a snapshot downloaded with aria2c
    download_dir=$(mktemp --directory)
    aria2c https://forest.chainsafe.io/calibnet/snapshot-latest.car.zst --dir="$download_dir"
    snapshot_path=$(echo "$download_dir"/*) # Should expand to the (only) thing in the directory
    snapshot_name=$(basename "$snapshot_path")
    "$FOREST_CLI_PATH" --chain calibnet snapshot intern "$snapshot_path" --snapshot-dir "$snapshot_dir"
    : snapshot should appear in list of snapshots
    "$FOREST_CLI_PATH" snapshot list --snapshot-dir "$snapshot_dir" \
        | grep --fixed-strings "$snapshot_name"
    clean-snapshot-dir

