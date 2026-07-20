---
sidebar_position: 1
title: Filecoin Node Comparison
---

# Filecoin Node Comparison

There are several full-node implementations of the Filecoin protocol.

- **[Forest](https://github.com/ChainSafe/forest)** is written in Rust and maintained by [ChainSafe Systems](https://chainsafe.io). It focuses on chain validation, a high-performance RPC API, and snapshot generation with low hardware requirements.
- **[Lotus](https://github.com/filecoin-project/lotus)** is the reference implementation, written in Go. It provides the complete Filecoin feature set, including the storage-provider stack, and is typically where new protocol features land first.
- **[Venus](https://github.com/filecoin-project/venus)** is a modular Go implementation. Rather than a single daemon, it is a set of independently deployable components (`venus`, `damocles`, `sophon-miner`, `venus-wallet`, `sophon-messager`, `sophon-auth`, `sophon-gateway`) that together pioneered Filecoin's distributed storage pool model, with a largely Lotus-compatible API. It aims to help small and medium-sized storage providers join the Filecoin network with a lower barrier to entry.

## Feature comparison

| Capability                                                        | Forest        | Lotus |
| ----------------------------------------------------------------- | ------------- | ----- |
| Chain synchronization and validation                              | Yes           | Yes   |
| Filecoin JSON-RPC API                                             | Yes           | Yes   |
| Ethereum-compatible RPC (`eth_*`)                                 | Yes           | Yes   |
| Snapshot export                                                   | Yes           | Yes   |
| Built-in and standalone wallet (`forest-wallet` / `lotus-wallet`) | Yes           | Yes   |
| Bootstrap node                                                    | Yes           | Yes   |
| F3 (Fast Finality) participation                                  | Yes           | Yes   |
| Built-in health-check endpoints                                   | Yes           | Yes   |
| Storage provider / sealing (`lotus-miner`, `lotus-worker`)        | No            | Yes   |
| Block production / mining                                         | No (untested) | Yes   |
| Single binary for all networks (`--chain`)                        | Yes           | No    |
| Auto-download snapshot on startup                                 | Yes           | No    |
| Lite / diff / archival snapshot generation                        | Yes           | No    |
| Offline RPC server from snapshot (`forest-tool api serve`)        | Yes           | No    |
| Stateless node mode (`--stateless`)                               | Yes           | No    |
| Ethereum `trace_call` and `debug_traceTransaction`                | Yes           | No    |
| Written in Rust 🦀                                                | Yes           | No    |

Forest and Lotus both expose a compatible Filecoin JSON-RPC API (requests and responses use the same JSON format). Forest serves three API versions: `/rpc/v0` (deprecated, legacy Lotus-compatible methods), `/rpc/v1` (stable and recommended for production), and `/rpc/v2` (experimental, still being rolled out). See the [JSON-RPC overview](../../reference/json-rpc/overview.md), [methods reference](../../reference/json-rpc/methods.mdx), and [API conformance report](../api_conformance/report_2026-07-01.md) for supported methods and Lotus API parity.

## Performance

For a comparable RPC workload, Forest served requests at lower latency while using less CPU and memory than Lotus, and it exports a snapshot significantly faster and with lower hardware requirements. See the [RPC Performance Comparison](./rpc_comparison.md) and [Snapshot Generation Comparison](./snapshot_comparison.md) for the full figures and methodology.
