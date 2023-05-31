#!/usr/bin/env bash
# This script is used to test the `forest-cli` commands that do not
# require a running `forest` node.
# It depends on the `forest-cli` binary being in the PATH.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

snapshot_dir=$TMP_DIR/snapshots

# return the first snapshot path listed by forest
function pop-snapshot-path() {
    forest-cli snapshot list --snapshot-dir "$snapshot_dir" \
        | grep path \
        | sed 's/^\tpath: //' \
        | head --lines=1
}


"$FOREST_CLI_PATH" fetch-params --keys

: fetch snapshot
"$FOREST_CLI_PATH" --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir"

: clean snapshots
"$FOREST_CLI_PATH" --chain calibnet snapshot clean --snapshot-dir "$snapshot_dir"

: "clean database (twice)"
"$FOREST_CLI_PATH" --chain calibnet db clean --force
"$FOREST_CLI_PATH" --chain calibnet db clean --force

: validate calibnet snapshot
"$FOREST_CLI_PATH" --chain calibnet snapshot clean --snapshot-dir "$snapshot_dir"

    : : fetch a calibnet snapshot
    "$FOREST_CLI_PATH" --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir"
    validate_me=$(pop-snapshot-path)

    : : validating under calibnet chain should succeed
    "$FOREST_CLI_PATH" --chain calibnet snapshot validate "$validate_me" --force

    : : validating under mainnet chain should fail
    if "$FOREST_CLI_PATH" --chain mainnet snapshot validate "$validate_me" --force; then
        exit 1
    fi

    : : test cleanup
    "$FOREST_CLI_PATH" snapshot clean --snapshot-dir "$snapshot_dir"

: intern a snapshot downloaded with aria2c
dir=$(mktemp --directory)
aria2c https://forest.chainsafe.io/calibnet/snapshot-latest.car.zst --dir="$dir"
snapshot_path=$(echo "$dir"/*)
snapshot_name=$(basename "$snapshot_path")
"$FOREST_CLI_PATH" --chain calibnet snapshot intern "$snapshot_path"
"$FOREST_CLI_PATH" snapshot list | grep --fixed-strings "$snapshot_name"


