#!/usr/bin/env bash

hyperfine \
  --runs 3 \
  --prepare 'sync; echo 1 | sudo tee /proc/sys/vm/drop_caches' \
  --cleanup 'cargo cli db clean --force' \
  --parameter-list CHUNK_SIZE 1000,5000,10000,20000,40000,200000 \
  --parameter-list BUFFER_CAPACITY 0,1,2,3 \
  --export-markdown db_tune_params.md \
  --command-name 'forest-import' \
  'CHUNK_SIZE={CHUNK_SIZE} BUFFER_CAPACITY={BUFFER_CAPACITY} cargo run --bin forest --release -- --chain mainnet --rpc false --no-gc --encrypt-keystore false --track-peak-rss --halt-after-import --import-snapshot filops_snapshot_mainnet_2023-07-02_height_2998800.car.zst'
