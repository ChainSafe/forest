#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

format="${1:-v1}"

source "$(dirname "$0")/harness.sh"

forest_init "$@"

retries=30
sleep_interval=0.5

echo "Cleaning up the initial snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

output=$($FOREST_CLI_PATH snapshot export-status --format json)
is_exporting=$(echo "$output" | jq -r '.exporting')
echo "Testing that no export is in progress"
if [ "$is_exporting" == "true" ]; then
  exit 1
fi

echo "Exporting zstd compressed snapshot in $format format"
$FOREST_CLI_PATH snapshot export --format "$format" > snapshot_export.log 2>&1 &
echo "Testing that export is in progress"
for ((i=1; i<=retries; i++)); do
    output=$($FOREST_CLI_PATH snapshot export-status --format json)
    is_exporting=$(echo "$output" | jq -r '.exporting')
    if [ "$is_exporting" == "true" ]; then
        break
    fi
    if [ $i -eq $retries ]; then
        cat snapshot_export.log
        exit 1
    fi
    sleep $sleep_interval
done

$FOREST_CLI_PATH snapshot export-cancel

echo "Testing that export has been cancelled"
for ((i=1; i<=retries; i++)); do
    output=$($FOREST_CLI_PATH snapshot export-status --format json)
    is_exporting=$(echo "$output" | jq -r '.exporting')
    is_cancelled=$(echo "$output" | jq -r '.cancelled')
    if [ "$is_exporting" == "false" ] && [ "$is_cancelled" == "true" ]; then
        break
    fi
    if [ $i -eq $retries ]; then
        cat snapshot_export.log
        exit 1
    fi
    sleep $sleep_interval
done

echo "Exporting zstd compressed snapshot at genesis"
$FOREST_CLI_PATH snapshot export --tipset 0 --format "$format"

echo "Exporting zstd compressed snapshot in $format format"
$FOREST_CLI_PATH snapshot export --format "$format" --tipset-lookup -o snapshot.forest.car.zst &
EXPORT_CMD_PID=$!
sleep 5
# another export job should be disallowed
export_error=$($FOREST_CLI_PATH snapshot export 2>&1 || true)
if echo "$export_error" | grep -q "active chain export job has started"; then
    echo "verified another export job is disallowed"
else 
    echo "another export job should be disallowed"
    echo "output was: $export_error"
    exit 1
fi
# another export-diff job should be disallowed
export_diff_error=$($FOREST_CLI_PATH snapshot export-diff --from 11000 --to 10100 -d 900 2>&1 || true)
if echo "$export_diff_error" | grep -q "active chain export job has started"; then
    echo "verified another export-diff job is disallowed"
else 
    echo "another export-diff job should be disallowed"
    echo "output was: $export_diff_error"
    exit 1
fi
# Killing the CLI should not cancel the export
echo "killing cli command"
kill -KILL $EXPORT_CMD_PID
# Wait on the same export job
echo "waiting on export-status"
$FOREST_CLI_PATH snapshot export-status --wait

$FOREST_CLI_PATH shutdown --force

# Check file sizes
ls -ahl *.forest.car.zst
# Validate tipset lookup snapshots
# export and check augmented data once we have receipts and events tipset published and imported
# for now there's no receipts and events on a freshly bootstrapped node.
forest-tool snapshot validate-extended --base snapshot.forest.car.zst --tipset-lookup snapshot_tipset_lookup.forest.car.zst
# Remove tipset lookup snapshots before proceeding 
rm *_tipset_lookup.forest.car.zst

for f in *.car.zst; do
  echo "Inspecting archive info $f"
  $FOREST_TOOL_PATH archive info "$f"
  echo "Inspecting archive metadata $f"
  $FOREST_TOOL_PATH archive metadata "$f"
done

echo "Testing snapshot validity"
zstd --test ./*.car.zst

echo "Verifying snapshot checksum"
sha256sum --check ./*.sha256sum

for f in *.car.zst; do
  echo "Validating CAR file $f"
  $FOREST_TOOL_PATH snapshot validate "$f"
done
