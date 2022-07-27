#!/bin/bash

set -e

ls $BASE_FOLDER
S3_FOLDER=$BASE_FOLDER/s3

RECENT_SNAPSHOT=$S3_FOLDER/calibnet/`ls -Atr1 $S3_FOLDER/calibnet/ | tail -n 1`

echo "Recent snapshot: $RECENT_SNAPSHOT"
forest --encrypt-keystore false --import-snapshot $RECENT_SNAPSHOT&

sleep 10


