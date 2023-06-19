---
title: Create new snapshot checkpoints
labels: ["Enhancement", "Priority: 3 - Medium"]
---

Forest uses checkpoints to improve performance when loading a snapshot. Without checkpoints, the blockchain has to be fully traversed to verify we have the right genesis block. Checkpoints short-circuit this search and shave off tens of minutes in boot time.

Checkpoints have to be regularly updated, though, and this issue is automatically created once per month. To close this issue, follow the procedure for computing new checkpoint hashes and add them to the checkpoints yaml file.

How to compute a new checkpoint for calibnet:

1. Install Forest and connect to the calibnet: `forest --chain calibnet --encrypt-keystore false`
2. Wait for Forest to catch up to the network: `forest-cli sync wait`
3. Compute new checkpoint hash: `forest-cli chain tipset-hash`
4. Add the checkpoint hash to the checkpoint [yaml file][yaml].

For mainnet, run the same commands but use `--chain mainnet` instead of `--chain calibnet`.

Issue TODOs:

- [ ] Add calibnet checkpoint to the [yaml file][yaml].
- [ ] Add mainnet checkpoint to the [yaml file][yaml].

[yaml]: https://github.com/ChainSafe/forest/blob/main/blockchain/chain/src/store/known_checkpoints.yaml
