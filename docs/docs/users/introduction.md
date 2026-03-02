---
title: Introduction
hide_title: true
sidebar_position: 1
slug: /
---

<p align="center">
  <img src="/img/logo-with-text.png" alt="Forest logo"/>
</p>

## What Is Forest?

Forest is a Filecoin full node written in Rust, by [ChainSafe Systems](https://chainsafe.io).

Forest focuses on four key properties:

- **Stability**: Forest is engineered for dependability. We prioritize stable interfaces, low maintenance, and consistent reliability, ensuring that node operators can trust in Forest's performance day in and day out.
- **Performance**: Understanding the importance of accessibility in the Filecoin network, we've optimized Forest for efficiency. Our goal is to lower the barriers to entry for network participants, making it more inclusive and expanding the ecosystem.
- **Security**: As a fundamental component of the network's infrastructure, the security of both the application and its development process cannot be overstated. Forest is developed with the highest security standards in mind, ensuring a secure environment for all users.
- **Ease of Use**: While managing a node comes with inherent complexities, we strive to make the experience as straightforward as possible. Our aim is for Forest to be approachable for both newcomers and advanced users, minimizing the learning curve and making node operation less cumbersome.

## Features

With Forest you can:

- Synchronize and interact with a Filecoin chain on all supported networks
- Import/export snapshots
- Submit transactions to the network
- Run a JSON-RPC server
- Act as a bootstrap node
- manage and interact with the FIL wallet

## Interacting with Forest

Forest consists of multiple binaries:

- `forest` - runs the Forest daemon, which synchronizes with the chain
- `forest-cli` - Interact with the daemon via RPC interface
- `forest-tool` - Utilities for maintaining and debugging Forest
- `forest-wallet` - Manage the built-in wallet

Check out the [CLI docs](./reference/cli) for more details.

## Roadmap Updates

Checkout [Github Discussions](https://github.com/ChainSafe/forest/discussions/categories/announcements) for monthly updates and roadmap announcements.

## Connect with Us

- Bug reports and feature requests: [Open an issue on Github](https://github.com/ChainSafe/forest/issues/new/choose)
- Questions, Comments, Feedback:
  - [Filecoin Slack](https://filecoin.io/slack): `#fil-forest-help`, `#fil-forest-dev` or `#fil-forest-announcements`
  - [Forest Github Discussions](https://github.com/ChainSafe/forest/discussions)
- Partnerships or Hand-on Support: forest (at) chainsafe [dot] io

## Contributing

Forest welcomes external contributions. Please review the contributing guidelines, and the [developer
documentation](/developers).
