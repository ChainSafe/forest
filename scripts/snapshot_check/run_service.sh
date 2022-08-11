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
check_env "SLACK_HOOK"

if [ "$error" -ne "0" ]; then
    echo "Please set the required environment variables and try again."
    exit 1
fi



# 1. Setup s3fs to get the snapshots.
# 2. Run forest script with docker-compose.

## Setup s3
mkdir --parents "$S3_FOLDER"
function cleanup {
  echo "unmounting s3 folder"
  fusermount -u -q "$S3_FOLDER"
}
trap cleanup EXIT
s3fs forest-snapshots "$S3_FOLDER" \
    -o url=https://fra1.digitaloceanspaces.com/ \
    -o allow_other

docker-compose up
