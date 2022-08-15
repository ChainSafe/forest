#!/bin/bash

set -e

apt-get install -y curl

report() {
    curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"❌  $CHAIN_NAME snapshot export failure!\"}" "$$SLACK_HOOK"
}
trap 'report' ERR

cd $BASE_FOLDER
RECENT_SNAPSHOT=s3/calibnet/`ls -Atr1 s3/calibnet/ | tail -n 1`
forest --encrypt-keystore false --metrics-address 0.0.0.0:6116 --chain $CHAIN_NAME --import-snapshot $${RECENT_SNAPSHOT} &

sleep 1200 # Wait for the RPC endpoint to be available. Remove this once Forest support the --detach flag.

forest sync wait # Wait for forest node to be completely synced.
echo "Synced to calibnet"

if [[ "$$(date -r "$${RECENT_SNAPSHOT}" +%F)" != "$$(date +%F)" ]]; then
    echo "No recent snapshot. Exporting new snapshot."
    forest chain export
    echo "Export done. Uploading.."
    mv ./forest_snapshot* s3/calibnet/
    echo "Upload done."
    curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"✅ $FOREST_HOSTNAME snapshot uploaded! 💪🌲!\"}" "$$SLACK_HOOK"
else
    echo "We already have a snapshot for today. Skipping."
    curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"✅ $FOREST_HOSTNAME snapshot check passed! 💪🌲!\"}" "$$SLACK_HOOK"
fi
