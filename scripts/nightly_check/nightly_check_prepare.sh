#!/usr/bin/env bash

# Exit codes
RET_CLONE_FAILED=1
RET_COMPILATION_FAILED=2
RET_RUN_FOREST_FAILED=3

# Clone latest `Forest` main.
if cd "$CHECK_DIR" && git clone --single-branch --branch main https://github.com/ChainSafe/forest.git && cd forest; then
  echo "✅ Repository cloned"
else
  echo "❌ Failed to clone the repository"
  exit "$RET_CLONE_FAILED"
fi

# Build the release binary
if cargo build --release; then
  echo "✅ Forest successfully compiled"
else
  echo "❌ Failed to build the binary"
  exit "$RET_COMPILATION_FAILED"
fi

# Launch Forest in background
LATEST_SNAPSHOT=$(find "$SNAPSHOT_DIR" -type f -name "*.car" -printf '%T@ %p\n' | sort -n | tail -1 | cut -f2- -d" ")
ulimit -n 8192

if target/release/forest --target-peer-count 50 --encrypt-keystore false --import-snapshot "$LATEST_SNAPSHOT" > "$LOG_FILE_RUN" 2>&1 &
then
  echo "✅ Successfully launched Forest in the background"
else
  echo "❌ Failed to launch Forest in the background"
  exit "$RET_RUN_FOREST_FAILED"
fi
