#!/usr/bin/env bash
# Runs in the `init` service: imports the snapshot ./resolve.rb picked, along
# with its extended snapshots (receipts and events, tipset lookup), before the
# daemon starts.

set -euo pipefail

url=$(< /data/snapshot-url)
read -r chain _ < /data/check-target

forest --chain="${chain}" --encrypt-keystore=false --import-snapshot="${url}" --halt-after-import
