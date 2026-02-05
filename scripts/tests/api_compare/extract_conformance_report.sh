#!/bin/bash

# This script extracts the conformance report from the finished compose volume.
# Usage: ./extract_conformance_report.sh <output_path>

set -euo pipefail

OUTPUT_PATH="$1"

# Volume is in the `node-data` volume
VOLUME_NAME="api_compare_node-data"
TEMP_CONTAINER_NAME="temp-extract-container"

# Create a temporary container to access the volume
docker run --name "$TEMP_CONTAINER_NAME" -v "$VOLUME_NAME":/data:ro -d alpine sleep infinity
trap 'docker rm -f "$TEMP_CONTAINER_NAME"' EXIT

REPORT_PATH_IN_VOLUME=$(docker exec "$TEMP_CONTAINER_NAME" sh -c 'find /data/api-compare-report -name "*.json" | head -n 1')

# Ensure the report file was found
if [ -z "$REPORT_PATH_IN_VOLUME" ]; then
  echo "Conformance report not found in the volume."
  exit 1
fi

# Copy the conformance report from the volume to the specified output path
docker cp "$TEMP_CONTAINER_NAME:$REPORT_PATH_IN_VOLUME" "$OUTPUT_PATH"
