#!/bin/sh

# Mounts Digital Ocean space for snapshots to /opt/snapshots
# Important:
# - credentials need to be put in .passwd-s3fs (in form key:secret)
# - /etc/fuse.conf need to have `user_allow_other` uncommented in order to mount it directly with docker

s3fs forest-snapshots /opt/snapshots \
-o passwd_file="${HOME}"/.passwd-s3fs \
-o url=https://fra1.digitaloceanspaces.com/ \
-o multipart_size=1000 \
-o allow_other

