#!/bin/bash

BASE_FOLDER=/tmp/forest-iac-snapshots

docker run \
    --device /dev/fuse \
    --cap-add SYS_ADMIN \
    --security-opt "apparmor=unconfined" \
    --env-file .env \
    --env=BASE_FOLDER="$BASE_FOLDER" \
    --rm \
    --detach \
    --label com.centurylinklabs.watchtower.enable=true \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -v $BASE_FOLDER:$BASE_FOLDER:rshared \
    ghcr.io/chainsafe/sync-snapshot
