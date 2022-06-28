<p align="center">
  <img width="380" height="269" src="./.github/forest_logo.png">
</p>

[![GitHub Workflow Status](https://img.shields.io/github/workflow/status/ChainSafe/forest/Rust?style=for-the-badge)](https://github.com/ChainSafe/forest/actions)
[![Codecov](https://img.shields.io/codecov/c/gh/ChainSafe/forest?style=for-the-badge&token=1OHO2CSD17)](https://codecov.io/gh/ChainSafe/forest)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/ChainSafe/forest?style=for-the-badge)](https://github.com/ChainSafe/forest/releases/latest)
[![License Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge)](https://opensource.org/licenses/Apache-2.0)
[![License MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)
[![Twitter](https://img.shields.io/twitter/follow/ChainSafeth.svg?style=for-the-badge&label=Twitter&color=1DA1F2)](https://twitter.com/ChainSafeth)
[![Discord](https://img.shields.io/discord/593655374469660673.svg?style=for-the-badge&label=Discord&logo=discord)](https://discord.gg/Q6A3YA2)

Forest is an implementation of [Filecoin](https://filecoin.io/) written in Rust. The implementation will take a modular approach to building a full Filecoin node in Rust from the [Filecoin Protocol Specification](https://filecoin-project.github.io/specs/), specifically the virtual machine, blockchain, and node system.

Our crates:

| component | description/crates |
| - | - |
| `forest` | the command-line interface and daemon (1 crate/workspace) |
| `node` | the networking stack and storage (7 crates) |
| `blockchain` | the chain structure and synchronization (6 crates) |
| `vm` | state transition and actors, messages, addresses (9 crates) |
| `key_management` | Filecoin account management (1 crate) |
| `crypto` | cryptographic functions, signatures, and verification (1 crate) |
| `encoding` | serialization library for encoding and decoding (1 crate) |
| `ipld` | the IPLD model for content-addressable data (9 crates) |
| `types` | the forest types (2 crates) |
| `utils` | the forest toolbox (12 crates) |


## Run with Docker

No need to install Rust toolchain or other dependencies, you will need only Docker.
```
❯ docker run --init -it ghcr.io/chainsafe/forest:latest --help
```

Follow other instructions for proper `forest` usage. You may need to mount a volume to import a snapshot, e.g.
```
❯ docker run --init -it -v ~/Downloads:/downloads ghcr.io/chainsafe/forest:latest --import-snapshot /downloads/minimal_finality_stateroots_latest.car
```

## Dependencies

* Rust `rustc >= 1.58.1`
* Rust WASM target `wasm32-unknown-unknown`

```shell
rustup install nightly
rustup target add wasm32-unknown-unknown
```

* OS Base-Devel/Build-Essential
* Clang compiler
* OpenCL bindings

```shell
# Ubuntu
sudo apt install build-essential clang ocl-icd-opencl-dev libssl-dev

# Archlinux
sudo pacman -S base-devel clang ocl-icd openssl
```

## Installation
```shell
# Clone repository
git clone --recursive https://github.com/chainsafe/forest
cd forest

# Install binary to $HOME/.cargo/bin and run node
make install
forest
```

To create release binaries, checkout the latest tag and compile with the release feature.
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/ChainSafe/forest?style=for-the-badge)](https://github.com/ChainSafe/forest/releases/latest)

```shell
git checkout $TAG
cargo build --release --bin forest --features release
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

Example of a [multiaddress](https://github.com/multiformats/multiaddr): `"/ip4/54.186.82.90/tcp/1347/p2p/12D3K1oWKNF7vNFEhnvB45E9mw2B5z6t419W3ziZPLdUDVnLLKGs"`

Forest will look for config files in the following order and priority:
 * Paths passed to the command line via the `--config` flag.
 * The environment variable `FOREST_CONFIG_PATH`, if no config was passed through command line arguments.
 * If none of the above are found, Forest will look in the systems default configuration directory (`$XDG_CONFIG_HOME` on Linux systems).
 * After all locations are exhausted and a config file is not found, a default configuration is assumed and used.

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

# To pull serialization vectors submodule and run serialization tests
make test-vectors

# To run all tests and all features enabled
make test-all
```

### Joining the testnet

Select the builtin calibnet configuration with the `--chain` option:

```bash
# Run and import past the state migrations to latest network version
./target/release/forest --chain calibnet --import-snapshot snapshot.car
```

Importing the snapshot only needs to happen during the first run. Following this, to restart the daemon run:

```bash
./target/release/forest --chain calibnet
```

### Interacting with Forest via CLI

When the Forest daemon is started, an admin token will be displayed. You will need this for commands that require a higher level of authorization (like a password). Forest, as mentioned above, uses multiaddresses for networking. This is no different in the CLI. To set the host and the port to use, if not using the default port or using a remote host, set the `FULLNODE_API_INFO` environment variable. This is also where you can set a token for authentication.

```
FULLNODE_API_INFO="<token goes here>:/ip4/<host>/tcp/<port>/http
```

Note that if a token is not present in the FULLNODE_API_INFO env variable, the colon is removed.

Forest developers will prepend this variable to CLI commands over using `export` on Linux or its equivalant on Windows. This will look like the following:

```
FULLNODE_API_INFO="..." forest auth api-info -p admin
```

### Documentation
_Work in progress_.
- https://chainsafe.github.io/forest/

## Contributing
- Check out our contribution guidelines: [CONTRIBUTING.md](documentation/developer_documentation/CONTRIBUTING.md)
- Have questions? Say _hi_ on [Discord](https://discord.gg/Q6A3YA2)!

## ChainSafe Security Policy

### Reporting a Security Bug

We take all security issues seriously, if you believe you have found a security issue within a ChainSafe
project please notify us immediately. If an issue is confirmed, we will take all necessary precautions
to ensure a statement and patch release is made in a timely manner.

Please email a description of the flaw and any related information (e.g. reproduction steps, version) to
[security at chainsafe dot io](mailto:security@chainsafe.io).

## License
Forest is dual licensed under [MIT](https://github.com/ChainSafe/forest/blob/main/LICENSE-MIT) + [Apache 2.0](https://github.com/ChainSafe/forest/blob/main/LICENSE-APACHE).
