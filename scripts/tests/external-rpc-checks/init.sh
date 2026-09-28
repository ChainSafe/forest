#!/usr/bin/env bash
# Runs in the `init` service: imports the snapshot ./resolve.rb picked and
# back-fills the chain index, all before the daemon starts.

set -euo pipefail

url=$(< /data/snapshot-url)
epoch=$(< /data/snapshot-epoch)
read -r chain start _ < /data/check-target

forest --chain="${chain}" --encrypt-keystore=false --import-snapshot="${url}" --halt-after-import

# inclusive on both ends
forest-tool index backfill --chain="${chain}" --from="${epoch}" --to="${start}"
