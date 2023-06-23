#!/usr/bin/env bash

export VALIDATE_FROM=400000
export VALIDATE_TO=450000


TRACE_FILE=parallel-$VALIDATE_FROM.json VALIDATION_METHOD=parallel cargo run --bin forest --release -- \
--encrypt-keystore=false \
--chain=calibnet \
--no-gc \
--import-snapshot=/home/aatif/chainsafe/snapshots/filecoin_full_calibnet_2023-04-07_450000.car \
--skip-load=true 


TRACE_FILE=legacy-$VALIDATE_FROM.json VALIDATION_METHOD=legacy cargo run --bin forest --release -- \
--encrypt-keystore=false \
--chain=calibnet \
--no-gc \
--import-snapshot=/home/aatif/chainsafe/snapshots/filecoin_full_calibnet_2023-04-07_450000.car \
--skip-load=true 
