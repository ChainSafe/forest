#!/bin/bash

set -e

if [ "$1" == "local" ]; then
  ENVIRONMENT="local"
elif [ "$1" == "docker" ]; then
  ENVIRONMENT="docker"
else
  echo "Usage: $0 <local|docker>"
  exit 1
fi

OUTPUT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/openrpc-specs"
mkdir -p "$OUTPUT_DIR"

echo "Generating OpenRPC specifications..."

if [ "$ENVIRONMENT" == "local" ]; then
  # Use local Forest binaries
  echo "Generating v0 spec..."
  forest-tool shed openrpc --path v0 > "$OUTPUT_DIR/v0.json"
  
  echo "Generating v1 spec..."
  forest-tool shed openrpc --path v1 > "$OUTPUT_DIR/v1.json"
  
  echo "Generating v2 spec..."
  forest-tool shed openrpc --path v2 > "$OUTPUT_DIR/v2.json"
else
  # Use Docker
  echo "Generating v0 spec..."
  docker run --rm --entrypoint forest-tool ghcr.io/chainsafe/forest:edge-fat shed openrpc --path v0 > "$OUTPUT_DIR/v0.json"
  
  echo "Generating v1 spec..."
  docker run --rm --entrypoint forest-tool ghcr.io/chainsafe/forest:edge-fat shed openrpc --path v1 > "$OUTPUT_DIR/v1.json"
  
  echo "Generating v2 spec..."
  docker run --rm --entrypoint forest-tool ghcr.io/chainsafe/forest:edge-fat shed openrpc --path v2 > "$OUTPUT_DIR/v2.json"
fi

echo "✓ Generated OpenRPC specifications in $OUTPUT_DIR"
echo "  - v0.json"
echo "  - v1.json"
echo "  - v2.json"
