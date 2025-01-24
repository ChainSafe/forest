---
title: Filecoin Services
hide_title: true
sidebar_position: 6
---

<p align="center" style="display: flex; justify-content: space-around;">
  <img src="/img/logo.png" alt="Forest logo"/>
  <img src="/img/chainsafe_logo.png" alt="ChainSafe logo"/>
  <img src="/img/filecoin_logo.png" alt="Filecoin logo"/>
</p>

ChainSafe works on various projects in the Filecoin ecosystem. While the Forest team focuses on improving the Forest client and collaborating with other Filecoin implementations to drive the network forward, ChainSafe's Infrastructure team provides a number of services to the Filecoin community.

## 🌲Forest

### Forest Node

Filecoin full node written in Rust. Actively-maintained repositories that are part of the Forest project are:

- [forest](https://github.com/ChainSafe/forest) - the central repository containing Forest node implementation, relevant tests and documentation.
- [forest-iac](https://github.com/ChainSafe/forest-iac) - Infrastructure as Code for deploying Forest nodes, mirroring important Filecoin assets and other services supporting Forest development.
- [fil-actor-states](https://github.com/ChainSafe/fil-actor-states) - state-only version of the [Filecoin actors](https://github.com/filecoin-project/builtin-actors), following semver versioning and providing a stable interface for Forest and other Filecoin implementations.

### Forest Explorer Faucet

Forest Explorer Faucet is a serverless application that allows users to request FIL on both mainnet and calibnet. The faucet is available at [forest-explorer.chainsafe.dev/faucet](https://forest-explorer.chainsafe.dev/faucet). The code is open source and available at [github.com/ChainSafe/forest-explorer](https://github.com/ChainSafe/forest-explorer).

:::info
Questions? Issues? Feedback? [Connect with the Forest team](./introduction.md#connect-with-us).
:::

## 🛠️ChainSafe Infrastructure

:::tip
You can check the status of many ChainSafe services at [status.chainsafe.dev](https://status.chainsafe.dev).
:::

### Bootstrap nodes

Bootstrap nodes are essential for new peers joining the network. They provide a list of known peers to connect to, allowing the new peer to join the network quickly. ChainSafe provides several bootstrap nodes (both Forest and Lotus-based) for the Filecoin network (the `chainsafe.io` domain):

- [calibnet](https://github.com/ChainSafe/forest/blob/main/build/bootstrap/calibnet)
- [mainnet](https://github.com/ChainSafe/forest/blob/main/build/bootstrap/mainnet)

### Latest Filecoin snapshots

The latest snapshots are required for new nodes to sync with the network. The snapshots are updated hourly and are available for both [mainnet](https://forest-archive.chainsafe.dev/list/mainnet/latest) and [calibnet](https://forest-archive.chainsafe.dev/list/calibnet/latest).

### Filecoin Archive

Filecoin Archive is a collection of Filecoin snapshots aiming to provide a historical record of the Filecoin network. The archive is available at [forest-archive.chainsafe.dev](https://forest-archive.chainsafe.dev).

### Calibnet FIL and datacap faucet

[Lotus Fountain](https://github.com/filecoin-project/lotus/blob/master/cmd/lotus-fountain/main.go)-based faucet for calibnet FIL and datacap. The faucet is available at [faucet.calibnet.chainsafe-fil.io](https://faucet.calibnet.chainsafe-fil.io).

### Storage

The S3-compatible IPFS/Filecoin gateway is available at [storage.chainsafe.io](https://storage.chainsafe.io).
