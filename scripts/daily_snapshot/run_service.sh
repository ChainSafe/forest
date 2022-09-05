#!/bin/bash

set -e

S3_FOLDER=$BASE_FOLDER/s3

error=0

# Check if an environment variable is set. If it isn't, set error=1.
check_env () {
    A="                        ";
    echo -n "${A:0:-${#1}} $1: "
    if [[ -z "${!1}" ]]; then
        echo "❌"
        error=1
    else
        echo "✅"
    fi
}

check_env "AWS_ACCESS_KEY_ID"
check_env "AWS_SECRET_ACCESS_KEY"
check_env "SLACK_API_TOKEN"
check_env "FOREST_SLACK_NOTIF_CHANNEL"

if [ "$error" -ne "0" ]; then
    echo "Please set the required environment variables and try again."
    exit 1
fi



# 1. Setup s3fs to get the snapshots.
# 2. Run forest script with docker-compose.

## Setup s3
umount "$S3_FOLDER" || true
mkdir --parents "$S3_FOLDER"

s3fs forest-snapshots "$S3_FOLDER" \
    -o default_acl=public-read \
    -o url=https://fra1.digitaloceanspaces.com/ \
    -o allow_other

cp -r ruby_common upload_snapshot.sh /scripts/
chmod +x /scripts/upload_snapshot.sh

docker-compose down
docker-compose up --build --force-recreate --detach
sleep infinity
