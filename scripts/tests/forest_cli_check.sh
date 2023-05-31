#!/usr/bin/env bash
# This script is used to test the `forest-cli` commands that do not
# require a running `forest` node.
# It depends on the `forest-cli` binary being in the PATH.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

function forest-cli () {
    "$FOREST_CLI_PATH" "$@"
}

snapshot_dir=$TMP_DIR/snapshots

# return the first snapshot path listed by forest
function pop-snapshot-path() {
    forest-cli snapshot list --snapshot-dir "$snapshot_dir" \
        | grep path \
        | sed 's/^\tpath: //' \
        | head --lines=1
}


forest-cli fetch-params --keys

: fetch snapshot
forest-cli --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir"

: clean snapshots
forest-cli --chain calibnet snapshot clean --snapshot-dir "$snapshot_dir"

: "clean database (twice)"
forest-cli --chain calibnet db clean --force
forest-cli --chain calibnet db clean --force

: validate calibnet snapshot
forest-cli --chain calibnet snapshot clean --snapshot-dir "$snapshot_dir"

: : fetch a calibnet snapshot
forest-cli --chain calibnet snapshot fetch --snapshot-dir "$snapshot_dir"
validate_me=$(pop-snapshot-path)

: : validating under calibnet chain should succeed
forest-cli --chain calibnet snapshot validate "$validate_me" --force

: : validating under mainnet chain should fail
if forest-cli --chain mainnet snapshot validate "$validate_me" --force; then
    exit 1
fi

: : test cleanup
forest-cli --chain fail snapshot clean --snapshot-dir "$snapshot_dir"

