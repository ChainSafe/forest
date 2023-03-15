<p align="center">
    <img width="380" height="269" src="./img/forest_logo.png">
</p>

[<img alt="build status" src="https://img.shields.io/circleci/build/gh/ChainSafe/forest/main?style=for-the-badge" height="20">](https://app.circleci.com/pipelines/github/ChainSafe/forest?branch=main)
[<img alt="GitHub release (latest by date)" src="https://img.shields.io/github/v/release/ChainSafe/forest?style=for-the-badge" height="20">](https://github.com/ChainSafe/forest/releases/latest)
[<img alt="Apache License" src="https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge" height="20">](https://opensource.org/licenses/Apache-2.0)
[<img alt="MIT License" src="https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge" height="20">](https://opensource.org/licenses/MIT)
[<img alt="Twitter" src="https://img.shields.io/twitter/follow/ChainSafeth.svg?style=for-the-badge&label=Twitter&color=1DA1F2" height="20">](https://twitter.com/ChainSafeth)
[<img alt="Discord" src="https://img.shields.io/discord/593655374469660673.svg?style=for-the-badge&label=Discord&logo=discord" height="20">](https://discord.gg/Q6A3YA2)

Forest is an implementation of [Filecoin](https://filecoin.io/) written in Rust.
The implementation takes a modular approach to building a full Filecoin node in
two parts — (i) building Filecoin’s security critical systems in Rust from the
[Filecoin Protocol Specification](https://filecoin-project.github.io/specs/),
specifically the virtual machine, blockchain, and node system, and (ii)
integrating functional components for storage mining and storage & retrieval
markets to compose a fully functional Filecoin node implementation.

## Functionality

- Filecoin State Tree Synchronization
- Filecoin JSON-RPC Server
- Ergonomic Message Pool
- Wallet CLI
- Process Metrics & Monitoring

## Disclaimer

The Forest implementation of the Filecoin protocol is alpha software which
should not yet be integrated into production workflows. The team is working to
provide reliable, secure, and efficient interfaces to the Filecoin ecosystem. If
you would like to chat, please reach out over Discord on the ChainSafe server
linked above.
