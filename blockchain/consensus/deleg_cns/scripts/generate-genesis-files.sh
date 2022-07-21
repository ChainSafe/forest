#!/usr/bin/env bash

set -e

OUT_DIR=$1

if [ -z "$OUT_DIR" ]; then
  echo "usage: ./generate-genesis-files.sh <output-dir>"
  exit 1
fi

lotus-seed --sector-dir "$OUT_DIR" pre-seal --miner-addr t01000
lotus-seed --sector-dir "$OUT_DIR" genesis new "$OUT_DIR/genesis.json"
lotus-seed --sector-dir "$OUT_DIR" genesis add-miner "$OUT_DIR/genesis.json" "$OUT_DIR/pre-seal-t01000.json"
lotus-seed --sector-dir "$OUT_DIR" genesis car --out "$OUT_DIR/genesis.car" "$OUT_DIR/genesis.json"

cd "$OUT_DIR"
find "." -type f \
  ! -name "pre-seal*" \
  ! -name "genesis*" \
  -exec rm -rf {} \;
cd -
