<p align="center">
  <img width="380" height="269" src="./.github/forest_logo.png">
</p>


[<img alt="build status" src="https://img.shields.io/circleci/build/gh/ChainSafe/forest/main?style=for-the-badge" height="20">](https://app.circleci.com/pipelines/github/ChainSafe/forest?branch=main)
[<img alt="Apache License" src="https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge" height="20">](https://opensource.org/licenses/Apache-2.0)
[<img alt="MIT License" src="https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge" height="20">](https://opensource.org/licenses/MIT)
[<img alt="Discord" src="https://img.shields.io/discord/593655374469660673.svg?style=for-the-badge&label=Discord&logo=discord" height="20">](https://discord.gg/Q6A3YA2)
[<img alt="Twitter" src="https://img.shields.io/twitter/follow/espadrine.svg?style=for-the-badge&label=Twitter&color=1DA1F2" height="20">](https://twitter.com/chainsafeth)


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
| `vm` | State transition and actors, message and address definitions |

## Dependencies
rustc >= 1.46.0

## Usage
```bash
# Clone repository
git clone --recursive https://github.com/chainsafe/forest
cd forest

# Install binary to $HOME/.cargo/bin and run node
make install
forest
```

> `OpenCL`, `hwloc` and a compatible assembly linker (ex. `clang`) are also required to build Filecoin proofs.

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

Example of a [multiaddress](https://github.com/multiformats/multiaddr): `"/ip4/54.186.82.90/tcp/1347/p2p/12D3K1oWKNF7vNFEhnvB45E9mw2B5z6t419W3ziZPLdUDVnLLKGs"`

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
cargo test # use `make test-release` for longer compilation but faster execution

# To pull serialization vectors submodule and run serialization and conformance tests
make test-vectors

# To run all tests and all features enabled
make test-all
```

### Joining the testnet

Build with the `interopnet` config with:

```bash
make interopnet

# Run and import past the state migrations to latest network version
./target/release/forest --import-snapshot ./types/networks/src/interopnet/snapshot.car
```

Importing the snapshot only needs to happen during the first run. Following this, to restart the daemon run:

```bash
./target/release/forest
```

### Documentation
https://chainsafe.github.io/forest/

## Contributing
- Check out our contribution guidelines: [CONTRIBUTING.md](CONTRIBUTING.md)  
- Have questions? Say hi on [Discord](https://discord.gg/Q6A3YA2)!

## ChainSafe Security Policy

### Reporting a Security Bug

We take all security issues seriously, if you believe you have found a security issue within a ChainSafe
project please notify us immediately. If an issue is confirmed, we will take all necessary precautions 
to ensure a statement and patch release is made in a timely manner.

Please email us a description of the flaw and any related information (e.g. reproduction steps, version) to
[security at chainsafe dot io](mailto:security@chainsafe.io).

## License 
Forest is dual licensed under [MIT](https://github.com/ChainSafe/forest/blob/main/LICENSE-MIT) + [Apache 2.0](https://github.com/ChainSafe/forest/blob/main/LICENSE-APACHE).
