#!/usr/bin/env bash
set -euo pipefail
CHAIN=calibnet

# https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/debug/filecoin_full_calibnet_2023-04-07_450000.car
SNAPSHOT=filecoin_full_calibnet_2023-04-07_450000.car
if [ ! -f $SNAPSHOT ]
then
    aria2c -x 4 "https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/debug/filecoin_full_calibnet_2023-04-07_450000.car"
fi

cargo build --release

# For some reason, cleaning the database with --cleanup gives me wildly inconsistent results.
hyperfine \
  --runs 5 \
  --parameter-list CHUNK_SIZE 1000,5000,10000,20000,40000,200000,500000 \
  --parameter-list BUFFER_CAPACITY 0,1,2,3 \
  --export-markdown db_tune_params.md \
  --command-name 'forest-import-{CHUNK_SIZE}-{BUFFER_CAPACITY}' \
    "echo \"[client]\nchunk_size = {CHUNK_SIZE}\nbuffer_size = {BUFFER_CAPACITY}\" > /tmp/forest.conf; \
    ./target/release/forest \
      --chain ${CHAIN} --config /tmp/forest.conf --rpc false --no-gc --encrypt-keystore false --halt-after-import \
      --import-snapshot ${SNAPSHOT}; \
    ./target/release/forest-cli --chain ${CHAIN} db clean --force"
