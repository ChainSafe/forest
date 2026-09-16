#!/usr/bin/env bash
# Runs in the `init` service: imports the snapshot ./resolve.rb picked and
# back-fills the chain index, all before the daemon starts.

set -euo pipefail

url=$(< /data/snapshot-url)
epoch=$(< /data/snapshot-epoch)

forest --chain=calibnet --encrypt-keystore=false --import-snapshot="${url}" --halt-after-import

# Indexes the 1000 epochs below the snapshot head, inclusive on both ends
forest-tool index backfill --chain=calibnet --from="${epoch}" --to="$((epoch - 1000))"
