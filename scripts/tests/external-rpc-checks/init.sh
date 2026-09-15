#!/usr/bin/env bash
# Runs in the `init` service in two steps, so that ./setup.sh can probe the
# dataset in between: `resolve` picks the snapshot to test against and records
# its head epoch, `import` imports it and back-fills the chain index, all before
# the daemon starts.

set -euo pipefail

# Records the URL and head epoch of the newest calibnet snapshot from DAYS_AGO
# days ago. Snapshot names end in the epoch of their head tipset; ./setup.sh
# reads it back to pick the range to check.
resolve() {
  # The Forest image ships neither curl nor jq.
  apt-get update -qq
  apt-get install -y -qq --no-install-recommends curl jq

  local day url epoch
  day=$(date -u -d "${DAYS_AGO:?} days ago" +%F)
  url=$(curl --silent --show-error --fail --retry 3 --connect-timeout 10 --max-time 60 \
    "https://forest-archive.chainsafe.dev/list/calibnet/latest-v2?format=json" |
    jq --raw-output --arg day "${day}" '[.items[].url | select(contains("_" + $day + "_"))] | first')
  [[ ${url} == https* ]] || {
    echo "no calibnet snapshot published for ${day}"
    exit 1
  }
  epoch=${url##*_height_}
  epoch=${epoch%%.*}
  printf '%s\n' "${url}" > /data/snapshot-url
  printf '%s\n' "${epoch}" > /data/snapshot-epoch
}

# Imports the resolved snapshot and indexes the 1000 epochs below its head,
# inclusive on both ends.
import() {
  local url epoch
  url=$(< /data/snapshot-url)
  epoch=$(< /data/snapshot-epoch)
  forest --chain=calibnet --encrypt-keystore=false --import-snapshot="${url}" --halt-after-import
  forest-tool index backfill --chain=calibnet --from="${epoch}" --to="$((epoch - 1000))"
}

case ${1:-} in
  resolve | import) "$1" ;;
  *)
    echo "usage: init.sh resolve|import"
    exit 2
    ;;
esac
