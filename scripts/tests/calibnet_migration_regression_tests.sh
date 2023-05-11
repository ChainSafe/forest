#!/usr/bin/env bash

set -e

# To test that migrations still work, we import a snapshot 100 epochs after the
# migration point and then we validate the last 200 tipsets. This triggers the
# migration logic without connecting to the real Filecoin network.

FOREST_PATH="forest"
MIGRATION_TEST="$FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --import-snapshot"

# NV17 - Shark, uncomment when we support the nv17 migration
# $MIGRATION_TEST "https://forest-snapshots.fra1.digitaloceanspaces.com/debug/filecoin_calibnet_height_16900.car.zst"

echo NV18 - Hygge
$MIGRATION_TEST "https://forest-snapshots.fra1.digitaloceanspaces.com/debug/filecoin_calibnet_height_322454.car.zst"

echo NV19 - Lightning
$MIGRATION_TEST "https://forest-snapshots.fra1.digitaloceanspaces.com/debug/filecoin_calibnet_height_489194.car.zst"

echo NV20 - Thunder # (no migration should happen in practice, it's a shadow upgrade). We test it anyway.
$MIGRATION_TEST "https://forest-snapshots.fra1.digitaloceanspaces.com/debug/filecoin_calibnet_height_492314.car.zst"
