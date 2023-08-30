#!/usr/bin/env bash
set -euxo pipefail

cargo run --bin forest-cli -- \
    snapshot compute-state \
    ~/chainsafe/snapshots/forest_snapshot_calibnet_2023-06-20_height_664544.car \
    --epoch 664532 \
    --json \
    > "output-$(git-short).json"
