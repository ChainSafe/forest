#!/usr/bin/env bash
# This script is checking the correctness of 
# the snapshot export feature.
# It requires both the `forest` and `forest-cli` binaries to be in the PATH.

set -eu

format="${1:-v1}"

source "$(dirname "$0")/harness.sh"

forest_init "$@"

echo "Cleaning up the initial snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

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

echo "Cleaning up the last snapshot"
rm --force --verbose ./*.{car,car.zst,sha256sum}

output=$($FOREST_CLI_PATH snapshot export-status --format json)
is_exporting=$(echo "$output" | jq -r '.exporting')
echo "Testing that no export is in progress"
if [ "$is_exporting" == "true" ]; then
  exit 1
fi

echo "Exporting zstd compressed snapshot at current tipset"
$FOREST_CLI_PATH snapshot export
sleep 1

output=$($FOREST_CLI_PATH snapshot export-status --format json)
is_exporting=$(echo "$output" | jq -r '.exporting')
echo "Testing that export is in progress"
if [ "$is_exporting" == "false" ]; then
  exit 1
fi

$FOREST_CLI_PATH snapshot export-cancel

output=$($FOREST_CLI_PATH snapshot export-status --format json)
is_exporting=$(echo "$output" | jq -r '.exporting')
is_cancelled=$(echo "$output" | jq -r '.cancelled')
echo "Testing that export has been cancelled"
if [ "$is_exporting" == "true" ] || [ "$is_cancelled" == "false" ]; then
  exit 1
fi
