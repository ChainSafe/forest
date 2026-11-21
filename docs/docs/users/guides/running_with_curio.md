---
title: Running Forest With Curio
sidebar_position: 99
---

[Curio](https://curiostorage.org/) is the successor to Lotus Miner. Forest can act as Curio's chain node (the same "Forest, Curio, YugabyteDB" layout Curio documents).

Verified on the local stack in [`scripts/devnet-forest-curio`](https://github.com/ChainSafe/forest/tree/main/scripts/devnet-forest-curio) against **Curio v1.29.0**:

- Put an admin token in `FULLNODE_API_INFO` and keep the miner worker key in Forest's wallet (`MinerCreateBlock` and message signing use it).
- On a private network, the genesis name must start with `local` or `test` (Curio drops other chain nodes). The stack uses `localnet`.

```shell
mise run test:devnet:miner:curio
```
