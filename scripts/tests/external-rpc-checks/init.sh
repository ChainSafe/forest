#!/usr/bin/env bash
# Runs in the `init` service: imports the snapshot ./resolve.rb picked and
# back-fills the chain index, all before the daemon starts.

set -euo pipefail

chain=${FOREST_CHAIN:-}
if [[ -z ${chain} ]]; then
  echo "FOREST_CHAIN is not set"
  exit 1
fi

epochs=${EPOCHS:-}
if [[ -z ${epochs} ]]; then
  echo "EPOCHS is not set"
  exit 1
fi

url=$(< /data/snapshot-url)
epoch=$(< /data/snapshot-epoch)

forest --chain="${chain}" --encrypt-keystore=false --import-snapshot="${url}" --halt-after-import

# Indexes the ${epochs} epochs below the snapshot head, inclusive on both ends
forest-tool index backfill --chain="${chain}" --from="${epoch}" --to="$((epoch - epochs))"
