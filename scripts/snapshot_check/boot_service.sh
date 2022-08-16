#!/bin/bash

set -o allexport
source .env
set +o allexport

#BASE_FOLDER=/tmp/forest-iac-snapshots

screen -S daily_snapshot -d -R \
    docker run \
    --device /dev/fuse \
    --cap-add SYS_ADMIN \
    --security-opt "apparmor=unconfined" \
    --env-file .env \
    --rm \
    --interactive --tty \
    --label com.centurylinklabs.watchtower.enable=true \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -v "$BASE_FOLDER":"$BASE_FOLDER":rshared \
    ghcr.io/chainsafe/sync-snapshot
