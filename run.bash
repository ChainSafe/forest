#!/usr/bin/env bash


snapshot=~/chainsafe/snapshots/forest_snapshot_calibnet_2023-06-29_height_690463.car
filename=$(basename $snapshot)
hyperfine \
    --warmup 1 \
    --export-markdown "$filename-results.md" \
    --export-json "$filename-results.json" \
    --parameter-list mode buffer8k,buffer1k,buffer100,unbuffered \
    --parameter-list snapshot "$snapshot" \
    './target/release/examples/benchmark --mode {mode} {snapshot}'

snapshot=~/chainsafe/snapshots/filecoin_full_calibnet_2023-04-07_450000.car
filename=$(basename $snapshot)

hyperfine \
    --runs 1 \
    --export-markdown "$filename-results.md" \
    --export-json "$filename-results.json" \
    --parameter-list mode buffer8k,buffer1k,buffer100,unbuffered \
    --parameter-list snapshot "$snapshot" \
    './target/release/examples/benchmark --mode {mode} {snapshot}'
