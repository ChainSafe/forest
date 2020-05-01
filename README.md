<p align="center">
  <img width="380" height="269" src="./.github/forest_logo.png">
</p>


![](https://github.com/ChainSafe/forest/workflows/Rust%20CI/badge.svg?event=push&branch=master)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Discord](https://img.shields.io/discord/593655374469660673.svg?label=Discord&logo=discord)](https://discord.gg/Q6A3YA2)
[![](https://img.shields.io/twitter/follow/espadrine.svg?label=Follow&style=social)](https://twitter.com/chainsafeth)


Forest is an implementation of [Filecoin](https://filecoin.io/) written in Rust. The implementation will take a modular approach to building a full Filecoin node in two parts — (i) building Filecoin’s security critical systems in Rust from the [Filecoin Protocol Specification](https://filecoin-project.github.io/specs/), specifically the virtual machine, blockchain, and node system, and (ii) integrating functional components for storage mining and storage & retrieval markets to compose a fully functional Filecoin node implementation.

❗**Current development should be considered a work in progress.**

Our crates:

| crate | description |
|-|-|
| `blockchain` | Chain structures and syncing functionality |
| `crypto` | Verification and signature definitions |
| `encoding` | Forest encoding and decoding |
| `ipld` | IPLD data model for content-addressable data |
| `node` | Networking synchronization and storage |
| `vm` | State transition and actor, message and address definitions |

## Dependencies
rustc >= 1.40.0

## Usage
```bash
# Clone repository
git clone https://github.com/chainsafe/forest
cd forest

# Install binary to $HOME/.cargo/bin and run node
make install
forest
```

### Config

Run the node with custom config and bootnodes

```bash
forest --config /path/to/your_config.toml
```

Example of config options available:

```toml
data_dir = "<directory for all chain and networking data>"
genesis_file = "<relative file path of genesis car file>"

[network]
listening_multiaddr = "<multiaddress>"
bootstrap_peers = ["<multiaddress>"]
```

Example of a [multiaddress](https://github.com/multiformats/multiaddr): `"/ip4/54.186.82.90/tcp/1347"`

### Logging

The Forest logger uses [Rust's log filtering options](https://doc.rust-lang.org/1.1.0/log/index.html#filtering-results) with the `RUST_LOG` environment variable.  
For example:

```bash
RUST_LOG="debug,forest_libp2p::service=info" forest
```

Will show all debug logs by default, but the `forest_libp2p::service` logs will be limited to `info`

### Testing
```bash
# To run base tests
cargo test # add --release flag for longer compilation but faster execution

# To pull serialization vectors submodule and run serialization tests
make test-vectors

# To run all tests and all features enabled
make test-all
```

### Documentation
https://chainsafe.github.io/forest/

## Contributing
- Check out our contribution guidelines: [CONTRIBUTING.md](CONTRIBUTING.md)  
- Have questions? Say hi on [Discord](https://discord.gg/Q6A3YA2)!

## License 
Forest is dual licensed under [MIT](https://github.com/ChainSafe/forest/blob/master/LICENSE-MIT) + [Apache 2.0](https://github.com/ChainSafe/forest/blob/master/LICENSE-APACHE).