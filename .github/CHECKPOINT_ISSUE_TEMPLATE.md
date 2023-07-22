---
title: Create new snapshot checkpoints
labels: ["Enhancement", "Priority: 3 - Medium"]
---

Forest uses checkpoints to improve performance when loading a snapshot. Without checkpoints, the blockchain has to be fully traversed to verify we have the right genesis block. Checkpoints short-circuit this search and shave off tens of minutes in boot time.

Checkpoints have to be regularly updated, though, and this issue is automatically created once per month. To close this issue, follow the procedure for computing new checkpoint hashes and add them to the checkpoints yaml file.

How to compute a new checkpoint for calibnet:

1. Install `forest-cli`
2. Download calibnet snapshot: `forest-cli --chain calibnet snapshot fetch`
3. Decompress snapshot: `zstd -d forest_snapshot_calibnet_*.car.zst`
4. Extract checkpoints: `forest-cli archive checkpoints forest_snapshot_calibnet_*.car`
5. Put checkpoints in `build/known_blocks.yaml`

For mainnet, run the same commands but use `--chain mainnet` instead of `--chain calibnet`.

Issue TODOs:

- [ ] Add calibnet checkpoint to the [yaml file][yaml].
- [ ] Add mainnet checkpoint to the [yaml file][yaml].

[yaml]: https://github.com/ChainSafe/forest/blob/main/blockchain/chain/src/store/known_checkpoints.yaml
