#!/usr/bin/env bash
# This script updates the snapshot checkpoints in build/known_blocks.yaml.
# It requires `forest-cli` binary to be in the PATH.

echo "
# This file maps epochs to block headers for calibnet and mainnet. Forest use
# this mapping to quickly identify the origin network of a tipset.
#
# Block headers can be inspected on filfox:
# - https://filfox.info/en/block/bafy2bzacebnfm6dvxo7sm5thcxnv3kttoamb53uxycnvtdxgk5mh7d73qlly2
# - https://calibration.filfox.info/en/block/bafy2bzacedhkkz76zdekpexha55b42eop42e24ajmajm26wws4nbvtq7louvu
#
# This file was generated with \\\`forest-cli archive checkpoints\\\`
# " > build/known_blocks.yaml

# import calibnet snapshot
forest-cli --chain calibnet snapshot fetch

# populate checkpoints for calibnet
forest-cli archive checkpoints "$(find . -name "forest_snapshot_calibnet*.forest.car.zst")" >> build/known_blocks.yaml

# forest-cli archive checkpoints ./forest_snapshot_mainnet_*.car
