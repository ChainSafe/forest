---
sidebar_position: 1
title: Forest vs Lotus
---

# Forest vs Lotus

Forest and Lotus are both full-node implementations of the Filecoin protocol. Forest is written in Rust and maintained by [ChainSafe Systems](https://chainsafe.io); it focuses on chain validation, a high-performance RPC API, and snapshot generation, exporting a chain snapshot substantially faster than Lotus while using a fraction of the memory. Lotus is written in Go; it provides the complete Filecoin feature set, including the storage-provider stack.

- Choose **Forest** to validate the chain, serve the Filecoin or Ethereum RPC API, generate snapshots, or run a bootstrap node, particularly where lower resource usage matters.
- Choose **Lotus** when you need storage-provider functionality (sealing, proving, block production) or the completeness of the reference implementation, where new protocol features typically land first.

## Feature comparison

| Capability                                                 | Forest | Lotus |
| ---------------------------------------------------------- | ------ | ----- |
| Chain synchronization and validation                       | Yes    | Yes   |
| Filecoin JSON-RPC API                                      | Yes    | Yes   |
| Ethereum-compatible RPC (`eth_*`)                          | Yes    | Yes   |
| Snapshot export                                            | Yes    | Yes   |
| Built-in wallet                                            | Yes    | Yes   |
| Bootstrap node                                             | Yes    | Yes   |
| F3 (Fast Finality) participation                           | Yes    | Yes   |
| Storage provider / sealing (`lotus-miner`, `lotus-worker`) | No     | Yes   |
| Block production / mining                                  | No     | Yes   |

## Performance

For a comparable RPC workload, Forest served requests at lower latency while using less CPU and memory than Lotus, and it exported a snapshot in a fraction of the time and memory. See the [RPC Performance Comparison](./rpc_comparison.md) and [Snapshot Generation Comparison](./snapshot_comparison.md) for the full figures and methodology.
