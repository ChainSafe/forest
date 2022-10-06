#!/bin/bash

# The .env file contains environment variables that we want access to.
set -o allexport
# Trust that the `.env` exists in the CWD during the script execution.
# shellcheck disable=SC1091
source .env
set +o allexport

error=0

# Check if an environment variable is set. If it isn't, set error=1.
check_env () {
    A="                            ";
    echo -n "${A:0:-${#1}} $1: "
    if [[ -z "${!1}" ]]; then
        echo "❌"
        error=1
    else
        echo "✅"
    fi
}

# Check that the environment variables in the .env file have been defined.
check_env "AWS_ACCESS_KEY_ID"
check_env "AWS_SECRET_ACCESS_KEY"
check_env "SLACK_API_TOKEN"
check_env "SLACK_NOTIF_CHANNEL"

if [ "$error" -ne "0" ]; then
    echo "Please set the required environment variables and try again."
    exit 1
fi

# With the access keys defined, let's run the snapshot generator. It requires
# fuse, SYS_ADMIN, and "apparmor=unconfined" in order to mount s3fs.
docker run \
    --device /dev/fuse \
    --cap-add SYS_ADMIN \
    --network host \
    --security-opt "apparmor=unconfined" \
    --env-file .env \
    --restart unless-stopped \
    --detach \
    --label com.centurylinklabs.watchtower.enable=true \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -v "$BASE_FOLDER":"$BASE_FOLDER":rshared \
    ghcr.io/chainsafe/sync-snapshot
