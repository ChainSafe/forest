#! /bin/sh
# This script downloads assets from DigitalOcean space on CI to save git-lfs bandwidth 

set -eu

SCRIPT_DIR=$(cd "$(dirname "$0")" ; pwd)

# Note: When updating the bundle, to not break CI on main
# do not replace `actor_bundles.car.zst` on DO space directly,
# upload another `actor_bundles_yyyy_MM_dd.car.zst`
curl -o "${SCRIPT_DIR}/actor_bundles.car.zst" https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/assets/actor_bundles.car.zst
