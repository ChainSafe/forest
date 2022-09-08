#!/bin/bash

SYNC_TIMEOUT=25m

CHAIN_NAME=$1
NEWEST_SNAPSHOT=$2

docker pull ghcr.io/chainsafe/forest:"${FOREST_TAG}"

COMMANDS="
echo \"Chain: $CHAIN_NAME\"
echo \"Snapshot: $NEWEST_SNAPSHOT\"
forest --encrypt-keystore false --chain $CHAIN_NAME --import-snapshot $NEWEST_SNAPSHOT --detach
timeout $SYNC_TIMEOUT forest sync wait
cat forest.err
cat forest.out
forest chain export
mv ./forest_snapshot* $BASE_FOLDER/s3/calibnet/
"

docker run \
  --rm \
  -v "$BASE_FOLDER":"$BASE_FOLDER":rshared \
  --entrypoint /bin/bash \
  ghcr.io/chainsafe/forest:"${FOREST_TAG}" \
  -c "$COMMANDS"
