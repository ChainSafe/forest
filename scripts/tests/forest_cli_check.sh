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

    : verify that we are byte-for-byte identical with filops
    zstd -d filops_*.car.zst
    "$FOREST_CLI_PATH" archive export filops_*.car -o exported_snapshot.car.zst
    zstd -d exported_snapshot.car.zst
    cmp --silent filops_*.car exported_snapshot.car

    : verify that the exported snapshot is in ForestCAR.zst format
    assert_eq "$(forest_query_format exported_snapshot.car.zst)" "ForestCARv1.zst"

    : verify that diff exports contain the expected number of state roots
    EPOCH=$(forest_query_epoch exported_snapshot.car.zst)
    "$FOREST_CLI_PATH" archive export --epoch $((EPOCH-1100)) --depth 900 --output-path base_snapshot.forest.car.zst exported_snapshot.car.zst

    BASE_EPOCH=$(forest_query_epoch base_snapshot.forest.car.zst)
    assert_eq "$BASE_EPOCH" $((EPOCH-1100))

    # This assertion is not true in the presence of null tipsets
    #BASE_STATE_ROOTS=$(forest_query_state_roots base_snapshot.forest.car.zst)
    #assert_eq "$BASE_STATE_ROOTS" 900

    "$FOREST_CLI_PATH" archive export --diff "$BASE_EPOCH" -o diff_snapshot.forest.car.zst exported_snapshot.car.zst
    # This assertion is not true in the presence of null tipsets
    #DIFF_STATE_ROOTS=$(forest_query_state_roots diff_snapshot.forest.car.zst)
    #assert_eq "$DIFF_STATE_ROOTS" 1100

    : Validate the union of a snapshot and a diff
    "$FOREST_CLI_PATH" snapshot validate --check-network calibnet base_snapshot.forest.car.zst diff_snapshot.forest.car.zst
rm -- *
popd



: validate latest calibnet snapshot
pushd "$(mktemp --directory)"
    : : fetch a compressed calibnet snapshot
    "$FOREST_CLI_PATH" --chain calibnet snapshot fetch
    test "$(num-files-here)" -eq 1
    uncompress_me=$(find . -type f | head -1)

    : : decompress it, as validate does not support compressed snapshots
    zstd --decompress --rm "$uncompress_me"

    validate_me=$(find . -type f | head -1)
    : : validating under calibnet chain should succeed
    "$FOREST_CLI_PATH" snapshot validate --check-network calibnet "$validate_me"

    : : validating under mainnet chain should fail
    if "$FOREST_CLI_PATH" snapshot validate --check-network mainnet "$validate_me"; then
        exit 1
    fi

    : : check that it contains at least one expected checkpoint
    # If calibnet is reset or the checkpoint interval is changed, this check has to be updated
    "$FOREST_CLI_PATH" archive checkpoints "$validate_me" | grep bafy2bzaceatx7tlwdhez6vyias5qlhaxa54vjftigbuqzfsmdqduc6jdiclzc
rm -- *
popd

