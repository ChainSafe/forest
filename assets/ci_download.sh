#! /bin/sh
# This script downloads assets from DigitalOcean space on CI to save git-lfs bandwidth 

set -eu

SCRIPT_DIR=$(cd "$(dirname "$0")" ; pwd)

curl -o "${SCRIPT_DIR}/actor_bundles.car.zst" https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/assets/actor_bundles.car.zst
