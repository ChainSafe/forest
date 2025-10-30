#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

format="${1:-v1}"

source "$(dirname "$0")/harness.sh"

forest_init "$@"

retries=10
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
$FOREST_CLI_PATH snapshot export --format "$format"

$FOREST_CLI_PATH shutdown --force

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
