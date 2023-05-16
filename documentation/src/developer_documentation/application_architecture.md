# The application architecture of `forest` largely mirrors that of `lotus`:

- There is a core
  [`StateManager`](https://github.com/ChainSafe/forest/blob/v0.8.2/blockchain/state_manager/src/lib.rs),
  which accepts:
  - [RPC calls](https://github.com/ChainSafe/forest/blob/v0.8.2/node/rpc/src/lib.rs)
  - [Filecoin peer state](https://github.com/ChainSafe/forest/blob/v0.8.2/blockchain/chain_sync/src/lib.rs)

For more information, see the
[lotus documentation](https://github.com/filecoin-project/lotus/blob/v1.23.0/documentation/en/architecture/architecture.md),
including, where relevant, the
[filecoin specification](https://spec.filecoin.io/).

(These also serve as a good introduction to the general domain, assuming a basic
familiarity with blockchains.)
