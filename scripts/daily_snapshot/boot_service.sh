#!/bin/bash

set -o allexport
source .env.bak
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

check_env "AWS_ACCESS_KEY_ID"
check_env "AWS_SECRET_ACCESS_KEY"
check_env "SLACK_API_TOKEN"
check_env "SLACK_NOTIF_CHANNEL"

if [ "$error" -ne "0" ]; then
    echo "Please set the required environment variables and try again."
    exit 1
fi

#screen -S daily_snapshot -d -R \
docker run \
    --device /dev/fuse \
    --cap-add SYS_ADMIN \
    --security-opt "apparmor=unconfined" \
    --env-file .env.bak \
    --network host \
    --interactive --tty \
    --label com.centurylinklabs.watchtower.enable=true \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -v "$BASE_FOLDER":"$BASE_FOLDER":rshared \
    --mount 'type=volume,src=scripts,dst=/scripts' \
    forest-snapshot:latest
    #ghcr.io/chainsafe/sync-snapshot
