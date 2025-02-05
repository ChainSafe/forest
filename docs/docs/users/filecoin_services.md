---
title: Filecoin Services
hide_title: true
sidebar_position: 6
---

<p align="center" style={{ display: 'flex' , justifyContent: 'space-around' }}>
  <img src="/img/logo.png" alt="Forest logo"/>
  <img src="/img/chainsafe_logo.png" alt="ChainSafe logo"/>
  <img src="/img/filecoin_logo.png" alt="Filecoin logo"/>
</p>

This page provides an overview of the services and infrastructure provided by ChainSafe across the Filecoin ecosystem.

:::tip
You can check the status of many ChainSafe services at [status.chainsafe.dev](https://status.chainsafe.dev).
:::

## Forest Node

Filecoin full node written in Rust aiming to provide a stable and performant implementation of the Filecoin protocol.

<details>

<summary>Forest repositories</summary>
<p>
Actively-maintained repositories that are part of the Forest project are:

- [forest](https://github.com/ChainSafe/forest) - the central repository containing Forest node implementation, relevant tests and documentation.
- [forest-iac](https://github.com/ChainSafe/forest-iac) - Infrastructure as Code for deploying Forest nodes, mirroring important Filecoin assets and other services supporting Forest development.
- [fil-actor-states](https://github.com/ChainSafe/fil-actor-states) - state-only version of the [Filecoin actors](https://github.com/filecoin-project/builtin-actors), following semver versioning and providing a stable interface for Forest and other Filecoin implementations.
</p>
</details>

## Forest Explorer Faucet

Forest Explorer Faucet is a serverless application that allows users to request FIL on both mainnet and calibnet. The faucet is available at [forest-explorer.chainsafe.dev/faucet](https://forest-explorer.chainsafe.dev/faucet). The code is open source and available at [github.com/ChainSafe/forest-explorer](https://github.com/ChainSafe/forest-explorer).

:::info
Questions? Issues? Feedback? [Connect with the Forest team](./introduction.md#connect-with-us).
:::

## Bootstrap nodes

Bootstrap nodes are essential for new peers joining the network. They provide a list of known peers to connect to, allowing the new peer to join the network quickly. ChainSafe provides the several bootstrap nodes (both Forest and Lotus-based) on Filecoin networks.

ChainSafe also operates an **archival bootstrap node** which maintains a full set of historical state to serve to the network. This is currently available for calibnet only.

### Calibnet

```
/dns/bootstrap-calibnet-0.chainsafe-fil.io/tcp/34000/p2p/12D3KooWABQ5gTDHPWyvhJM7jPhtNwNJruzTEo32Lo4gcS5ABAMm
/dns/bootstrap-calibnet-1.chainsafe-fil.io/tcp/34000/p2p/12D3KooWS3ZRhMYL67b4bD5XQ6fcpTyVQXnDe8H89LvwrDqaSbiT
/dns/bootstrap-calibnet-2.chainsafe-fil.io/tcp/34000/p2p/12D3KooWEiBN8jBX8EBoM3M47pVRLRWV812gDRUJhMxgyVkUoR48
/dns/bootstrap-archive-calibnet-0.chainsafe-fil.io/tcp/1347/p2p/12D3KooWLcRpEfmUq1fC8vfcLnKc1s161C92rUewEze3ALqCd9yJ
```

### Mainnet

```
/dns/bootstrap-mainnet-0.chainsafe-fil.io/tcp/34000/p2p/12D3KooWKKkCZbcigsWTEu1cgNetNbZJqeNtysRtFpq7DTqw3eqH
/dns/bootstrap-mainnet-1.chainsafe-fil.io/tcp/34000/p2p/12D3KooWGnkd9GQKo3apkShQDaq1d6cKJJmsVe6KiQkacUk1T8oZ
/dns/bootstrap-mainnet-2.chainsafe-fil.io/tcp/34000/p2p/12D3KooWHQRSDFv4FvAjtU32shQ7znz7oRbLBryXzZ9NMK2feyyH
```

Mainnet bootstrap nodes' status can be checked at [probelab.io/bootstrappers/filecoin](https://probelab.io/bootstrappers/filecoin).

## Latest Filecoin snapshots

The latest snapshots are required for new nodes to sync with the network. The snapshots are updated hourly and are available for both [mainnet](https://forest-archive.chainsafe.dev/list/mainnet/latest) and [calibnet](https://forest-archive.chainsafe.dev/list/calibnet/latest).

## Filecoin Archive

Filecoin Archive is a collection of Filecoin snapshots aiming to provide a historical record of the Filecoin network. The archive is available at [forest-archive.chainsafe.dev](https://forest-archive.chainsafe.dev).

## Calibnet FIL and datacap faucet

[Lotus Fountain](https://github.com/filecoin-project/lotus/blob/master/cmd/lotus-fountain/main.go)-based faucet for calibnet FIL and datacap. The faucet is available at [faucet.calibnet.chainsafe-fil.io](https://faucet.calibnet.chainsafe-fil.io).

## Storage

The S3-compatible IPFS/Filecoin gateway is available at [storage.chainsafe.io](https://storage.chainsafe.io).
