#!/bin/bash

set -e

S3_FOLDER=$BASE_FOLDER/s3

# 1. Setup s3fs to get the snapshots.
# 2. Make sure an instance of watchtower is running.
# 3. Run Ruby script for exporting and uploading a new snapshot
#    if there isn't one for today already.

## Setup s3
umount "$S3_FOLDER" || true
mkdir --parents "$S3_FOLDER"

s3fs forest-snapshots "$S3_FOLDER" \
    -o default_acl=public-read \
    -o url=https://fra1.digitaloceanspaces.com/ \
    -o allow_other

## Ensure watchtower is running
docker stop watchtower 2> /dev/null || true
docker wait watchtower 2> /dev/null || true
docker run --rm \
    --detach \
    -v /var/run/docker.sock:/var/run/docker.sock \
    --name watchtower \
    containrrr/watchtower \
    --label-enable --include-stopped --revive-stopped --stop-timeout 120s --interval 600

# Export and upload snapshot
ruby daily_snapshot.rb calibnet
