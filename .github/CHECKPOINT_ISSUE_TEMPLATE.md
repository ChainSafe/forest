---
title: "cron: update known_blocks.yaml"
labels: ["Type: Task"]
---

Forest uses checkpoints to improve performance when loading a snapshot. Without checkpoints, the blockchain has to be fully traversed to verify we have the right genesis block. Checkpoints short-circuit this search and shave off tens of minutes in boot time.

Checkpoints have to be regularly updated, though, and [this issue](/.github/CHECKPOINT_ISSUE_TEMPLATE.md) is [automatically created once per month](/.github/workflows/checkpoints.yml). Follow the procedure below to update [`build/known_blocks.yaml`](/build/known_blocks.yaml), and close this issue.

# Procedure

```bash
#!/bin/bash

# Perform this for `calibnet` AND `mainnet`
chains=("mainnet" "calibnet")

for chain in "${chains[@]}"
do
    # download the latest snapshot.
    # =============================
    # - calibnet ~6G, ~5min on a droplet
    # - mainnet ~74G, ~60mins on a droplet
    aria2c -x5 https://forest-archive.chainsafe.dev/latest/"$chain"/ -o "$chain"

    # print out the checkpoints.
    # ==========================
    # The whole operation takes a long time, BUT you only need the first line or so.
    timeout 15s forest-tool archive checkpoints "$chain"
done

# Update `build/known_blocks.yaml` as appropriate, manually.
```
