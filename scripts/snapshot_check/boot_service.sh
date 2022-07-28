#!/bin/bash

BASE_FOLDER=/tmp/forest-iac-snapshots
FOREST_HOST_LOGS=$(pwd)/logs

mkdir --parents $FOREST_HOST_LOGS

docker run \
    --device /dev/fuse \
    --cap-add SYS_ADMIN \
    --security-opt "apparmor=unconfined" \
    --env-file .env \
    --env=BASE_FOLDER=$BASE_FOLDER \
    --env=FOREST_HOST_LOGS=$FOREST_HOST_LOGS \
    --rm -it \
    --label com.centurylinklabs.watchtower.enable=true \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -v $BASE_FOLDER:$BASE_FOLDER:rshared \
    chainsafe/sync-snapshot
