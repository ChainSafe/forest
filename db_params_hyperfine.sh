#!/usr/bin/env bash

CHAIN=calibnet
SNAPSHOT=filecoin_full_calibnet_2023-04-07_450000.car

hyperfine \
  --runs 2 \
  --cleanup './target/release/forest-cli --chain ${CHAIN} db clean --force' \
  --parameter-list CHUNK_SIZE 1000,5000,10000,20000,40000,200000 \
  --parameter-list BUFFER_CAPACITY 0,1,2,3 \
  --export-markdown db_tune_params.md \
  --command-name 'forest-import-{CHUNK_SIZE}-{BUFFER_CAPACITY}' \
  'CHUNK_SIZE={CHUNK_SIZE} BUFFER_CAPACITY={BUFFER_CAPACITY} cargo run --bin forest --release -- --chain ${CHAIN} --rpc false --no-gc --encrypt-keystore false --track-peak-rss --halt-after-import --import-snapshot ${SNAPSHOT}'
