#!/usr/bin/env sh

CHECK_DIR="$HOME"/sync_check
SNAPSHOT_DIR="$CHECK_DIR"/snapshots

# Download to downloading
curl -sI https://fil-chain-snapshots-fallback.s3.amazonaws.com/mainnet/minimal_finality_stateroots_latest.car | perl -ne '/x-amz-website-redirect-location:\s(.+)\.car/ && print "$1.sha256sum\n$1.car"' | xargs wget --no-clobber --directory-prefix="$SNAPSHOT_DIR"
