---
title: "cron: update known_blocks.yaml"
labels: ["Enhancement", "Priority: 3 - Medium"]
---

Forest uses checkpoints to improve performance when loading a snapshot. Without checkpoints, the blockchain has to be fully traversed to verify we have the right genesis block. Checkpoints short-circuit this search and shave off tens of minutes in boot time.

Checkpoints have to be regularly updated, though, and [this issue](/.github/CHECKPOINT_ISSUE_TEMPLATE.md) is [automatically created once per month](/.github/workflows/checkpoints.yml). Follow the procedure below to update [`build/known_blocks.yaml`](/build/known_blocks.yaml), and close this issue.

# Procedure

```bash
# Perform this for `calibnet` AND `mainnet`
chain=calibnet

# download the latest snapshot.
# =============================
# - calibnet ~3G, ~1min on a droplet
# - mainnet ~60G, ~15mins on a droplet
aria2c https://forest-archive.chainsafe.dev/latest/$chain/ -o $chain

# print out the checkpoints.
# ==========================
# The whole operation takes a long time, BUT you only need the first line or so.
cargo run --bin forest-tool -- archive checkpoints $chain

# Update `build/known_blocks.yaml` as appropriate...
```
