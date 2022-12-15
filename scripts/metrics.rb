#!/usr/bin/env ruby

require 'toml-rb'

# Measure time to import Forest snapshot
# TODO: automate process of finding correct snapshot and remove hard-coded snapshot path, may need to change directories
system 'time forest --chain calibnet --encrypt-keystore false --import-snapshot forest_snapshot_calibnet_2022-12-14_height_121664.car --halt-after-import'
# TODO: save result

# Measure time to import Lotus snapshot
# TODO: automate process of finding correct snapshot and remove hard-coded snapshot path, may need to change directories
system 'time lotus daemon --import-snapshot filecoin_snapshot_calibnet_2022-12-14_height_123360.car --halt-after-import'
# TODO: save result

# Open new window separate from node for `sync` commands
system 'gnome-terminal'

# Run Forest node
system 'forest --chain calibnet --encrypt-keystore false --import-snapshot forest_snapshot_calibnet_2022-12-14_height_121664.car'
# Check stage and store height if at correct stage (in separate terminal window)
system 'forest-cli sync status'

# TODO: repeat for Lotus