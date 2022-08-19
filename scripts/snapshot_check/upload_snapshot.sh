#!/bin/bash

set -e

DETACH_TIMEOUT=10m
SYNC_TIMEOUT=25m
UPLOAD_INTERVAL=4h

apt-get install -y curl

report() {
    curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"❌  $CHAIN_NAME snapshot export failure!\"}" "$SLACK_HOOK"
}
trap 'report' ERR

cd "$BASE_FOLDER"

while true
do
  files=("$BASE_FOLDER"/s3/"$CHAIN_NAME"/*)
  NEWEST_SNAPSHOT=${files[0]}
  for f in "${files[@]}"; do
    if [[ $f -nt $NEWEST_SNAPSHOT ]]; then
      NEWEST_SNAPSHOT=$f
    fi
  done

  if [[ "$(date -r "$NEWEST_SNAPSHOT" +%F)" != "$(date +%F)" ]]; then
      forest --encrypt-keystore false --metrics-address 0.0.0.0:6116 --chain "$CHAIN_NAME" --import-snapshot "$NEWEST_SNAPSHOT" &
      FOREST_PID=$! 

      sleep "$DETACH_TIMEOUT" # Wait for the RPC endpoint to be available. Remove this once Forest support the --detach flag.

      timeout "$SYNC_TIMEOUT" forest sync wait # Wait for forest node to be completely synced.
      echo "Synced to calibnet"

      echo "No recent snapshot. Exporting new snapshot."
      forest chain export
      echo "Export done. Uploading.."
      mv ./forest_snapshot* s3/calibnet/
      echo "Upload done."
      curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"✅ $CHAIN_NAME snapshot uploaded! 💪🌲!\"}" "$SLACK_HOOK"

      kill "$FOREST_PID"
  else
      echo "We already have a snapshot for today. Skipping."
      # curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"✅ $CHAIN_NAME snapshot check passed! 💪🌲!\"}" "$SLACK_HOOK"
  fi

  sleep "$UPLOAD_INTERVAL"
done
