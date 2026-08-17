#!/usr/bin/env bash
# Runs in the `init` service: resolves the snapshot to test against, imports it
# and back-fills the chain index, all before the daemon starts. Keeping it here
# means the date handling is always Linux, whatever the host.

set -euo pipefail

# The Forest image ships neither curl nor jq.
apt-get update -qq
apt-get install -y -qq --no-install-recommends curl jq

# The dataset lags the chain, so yesterday's snapshot is the one to test against.
day=$(date -u -d '1 day ago' +%F)
url=$(curl --silent --show-error --fail --retry 3 --connect-timeout 10 --max-time 60 \
  "https://forest-archive.chainsafe.dev/list/calibnet/latest-v2?format=json" |
  jq --raw-output --arg day "${day}" '[.items[].url | select(contains("_" + $day + "_"))] | first')
[[ ${url} == https* ]] || {
  echo "no calibnet snapshot published for ${day}"
  exit 1
}

# Snapshot names end in the epoch of their head tipset. ./setup.sh reads it back
# to pick the range to check.
epoch=${url##*_height_}
epoch=${epoch%%.*}
printf '%s\n' "${epoch}" > /data/snapshot-epoch

forest --chain=calibnet --encrypt-keystore=false --no-gc \
  --height=-50 --import-snapshot="${url}" --halt-after-import

# Indexes the 1000 epochs below the snapshot head; `--from` counts back
# inclusively, so one extra tipset covers the whole range. The offline
# back-fill already recomputes missing state and indexes up to the head.
forest-tool index backfill --chain=calibnet --from="${epoch}" --n-tipsets=1001
