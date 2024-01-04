<p align="center">
    <img height="269" src="./img/forest_logo.png">
</p>

<p align="center">
    <a href="https://github.com/ChainSafe/forest/actions"><img alt="GitHub Workflow Status" src="https://img.shields.io/github/actions/workflow/status/ChainSafe/forest/forest.yml?style=for-the-badge"></a>
    <a href="https://github.com/ChainSafe/forest/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/ChainSafe/forest?style=for-the-badge"></a>
    <a href="https://docs.forest.chainsafe.io"><img alt="Docs" src="https://img.shields.io/badge/doc-user_guide-green?style=for-the-badge"></a>
    <a href="https://docs.forest.chainsafe.io/rustdoc/"><img alt="Rust Docs" src="https://img.shields.io/badge/doc-rust_docs-green?style=for-the-badge"></a>
</p>
 <p align="center">
    <a href="https://github.com/ChainSafe/forest/blob/main/LICENSE-APACHE"><img alt="License Apache 2.0" src="https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge"></a>
    <a href="https://github.com/ChainSafe/forest/blob/main/LICENSE-MIT"><img alt="License MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge"></a>
    <a href="https://discord.gg/Q6A3YA2"><img alt="Discord" src="https://img.shields.io/discord/593655374469660673.svg?style=for-the-badge&label=Discord&logo=discord"></a>
    <a href="https://twitter.com/ChainSafeth"><img alt="Twitter" src="https://img.shields.io/twitter/follow/chainsafeth?style=for-the-badge&color=1DA1F2"></a>
</p>

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
