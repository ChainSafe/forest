<p align="center">
  <img width="380" height="269" src="documentation/src/img/forest_logo.png">
</p>

[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/ChainSafe/forest/rust.yml?style=for-the-badge)](https://github.com/ChainSafe/forest/actions)
[![Codecov](https://img.shields.io/codecov/c/gh/ChainSafe/forest?style=for-the-badge&token=1OHO2CSD17)](https://codecov.io/gh/ChainSafe/forest)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/ChainSafe/forest?style=for-the-badge)](https://github.com/ChainSafe/forest/releases/latest)

<!-- [![dependency status](https://deps.rs/repo/github/ChainSafe/forest/status.svg?style=for-the-badge)](https://deps.rs/repo/github/ChainSafe/forest) -->

[![forest book](https://img.shields.io/badge/doc-book-green?style=for-the-badge)](https://chainsafe.github.io/forest/)
[![rustdoc@main](https://img.shields.io/badge/doc-rustdoc@main-green?style=for-the-badge)](https://chainsafe.github.io/forest/rustdoc/)
[![License Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge)](https://github.com/ChainSafe/forest/blob/main/LICENSE-APACHE)
[![License MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://github.com/ChainSafe/forest/blob/main/LICENSE-MIT)
[![Twitter](https://img.shields.io/twitter/follow/ChainSafeth.svg?style=for-the-badge&label=Twitter&color=1DA1F2)](https://twitter.com/ChainSafeth)
[![Discord](https://img.shields.io/discord/593655374469660673.svg?style=for-the-badge&label=Discord&logo=discord)](https://discord.gg/Q6A3YA2)

Forest is an implementation of [Filecoin](https://filecoin.io/) written in Rust.
The implementation will take a modular approach to building a full Filecoin node
in Rust from the
[Filecoin Protocol Specification](https://filecoin-project.github.io/specs/),
specifically the virtual machine, blockchain, and node system.

Our crates:

| component        | description/crates                                              |
| ---------------- | --------------------------------------------------------------- |
| `forest`         | the command-line interface and daemon (3 crate/workspace)       |
| `node`           | the networking stack and storage (7 crates)                     |
| `blockchain`     | the chain structure and synchronization (8 crates)              |
| `vm`             | state transition and actors, messages, addresses (9 crates)     |
| `key_management` | Filecoin account management (1 crate)                           |
| `crypto`         | cryptographic functions, signatures, and verification (1 crate) |
| `encoding`       | serialization library for encoding and decoding (1 crate)       |
| `ipld`           | the IPLD model for content-addressable data (9 crates)          |
| `types`          | the forest types (2 crates)                                     |
| `utils`          | the forest toolbox (12 crates)                                  |

## Questions

Have questions? Feel free to post them in
[Forest Q&A](https://github.com/ChainSafe/forest/discussions/categories/forest-q-a)!

## Run with Docker

No need to install Rust toolchain or other dependencies, you will need only
Docker - works on Linux, macOS and Windows.

```
# daemon
❯ docker run --init -it --rm ghcr.io/chainsafe/forest:latest --help
# cli
❯ docker run --init -it --rm --entrypoint forest-cli ghcr.io/chainsafe/forest:latest --help
```

For more in-depth usage and sample use cases, please refer to the Forest Docker
documentation in the [Forest Book](https://chainsafe.github.io/forest/). Keep in
mind that the `latest` tag is the latest stable release. If you want to current
development build, use the `edge` tag.

## Dependencies

- Rust (toolchain version is specified in `rust-toolchain.toml`)

Install [rustup](https://rustup.rs/)

- OS Base-Devel/Build-Essential
- Clang compiler

### Ubuntu (20.04)

```
sudo make install-deps
```

or

```
sudo apt install build-essential clang cmake
```

### Archlinux

```
sudo pacman -S base-devel clang
```

### Fedora (36)

```
sudo dnf install -y clang-devel cmake
```

### Alpine

```
apk add git curl make cmake gcc clang clang-dev musl-dev
```

## Installation

```shell
# Clone repository
git clone --recursive https://github.com/chainsafe/forest
cd forest

# Install binary to $HOME/.cargo/bin and run node
# avx2 is enabled by default. To disable it, remove
# -Ctarget-feature from RUSTFLAGS in .cargo/config.toml
make install

forest
```

To create release binaries, checkout the latest tag and compile with the release
feature.
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/ChainSafe/forest?style=for-the-badge)](https://github.com/ChainSafe/forest/releases/latest)

```shell
git checkout $TAG
make build # make debug build of forest daemon and cli
# or
make release # make release build of forest daemon and cli
# or
make install # install forest daemon and cli
```

### Config

#### Keystore

To encrypt the keystore while in headless mode, set the `FOREST_KEYSTORE_PHRASE`
environmental variable. Otherwise, skip the encryption (not recommended in
production environments) with `--encrypt-keystore false`.

#### Network

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

Example of a [multiaddress](https://github.com/multiformats/multiaddr):
`"/ip4/54.186.82.90/tcp/1347/p2p/12D3K1oWKNF7vNFEhnvB45E9mw2B5z6t419W3ziZPLdUDVnLLKGs"`

#### Configuration sources

Forest will look for config files in the following order and priority:

- Paths passed to the command line via the `--config` flag.
- The environment variable `FOREST_CONFIG_PATH`, if no config was passed through
  command line arguments.
- If none of the above are found, Forest will look in the systems default
  configuration directory (`$XDG_CONFIG_HOME` on Linux systems).
- After all locations are exhausted and a config file is not found, a default
  configuration is assumed and used.

### Logging

The Forest logger uses
[Rust's log filtering options](https://doc.rust-lang.org/1.1.0/log/index.html#filtering-results)
with the `RUST_LOG` environment variable. For example:

```bash
RUST_LOG="debug,forest_libp2p::service=info" forest
```

Will show all debug logs by default, but the `forest_libp2p::service` logs will
be limited to `info`

Forest can also send telemetry to the endpoint of a Loki instance or a Loki
agent (see [Grafana Cloud](https://grafana.com/oss/loki/)). Use `--loki` to
enable it and `--loki-endpoint` to specify the interface and the port.

### Testing

First, install the [`nextest`](https://nexte.st/) test runner.

```bash
cargo install cargo-nextest --locked
```

```bash
# To run base tests
cargo nextest run # use `make test-release` for longer compilation but faster execution

# To pull serialization vectors submodule and run serialization tests
make test-vectors

# To run all tests and all features enabled
make test-all
```

Chain synchronization checks are run after every merge to `main`. This code is
maintained in a separate repository -
[Forest IaC](https://github.com/ChainSafe/forest-iac).

### Linters

The project uses exhaustively a set of linters to keep the codebase clean and
secure in an automated fashion. While the CI will have them installed, if you
want to run them yourself before submitting a PR (recommended), you should
install a few of them.

```bash
# You can install those linters also with other package managers or by manually grabbing the binaries from the projects' repositories.

# Rust code linter
rustup component add clippy

# Rust code formatter
rustup component add rustfmt

# TOML linter
cargo install taplo-cli --locked

# Scanning dependencies for security vulnerabilities
cargo install cargo-audit

# Spellcheck
cargo install cargo-spellcheck
```

After everything is installed, you can run `make lint-all`.

### Joining the testnet

Select the builtin calibnet configuration with the `--chain` option. The
`--auto-download-snapshot` will ensure that a snapshot is downloaded if needed
without any prompts.

```bash
./target/release/forest --chain calibnet --auto-download-snapshot
```

### Interacting with Forest via CLI

When the Forest daemon is started, an admin token will be displayed
(alternatively, use `--save-token <token>` flag to save it on disk). You will
need this for commands that require a higher level of authorization (like a
password). Forest, as mentioned above, uses multiaddresses for networking. This
is no different in the CLI. To set the host and the port to use, if not using
the default port or using a remote host, set the `FULLNODE_API_INFO` environment
variable. This is also where you can set a token for authentication.

```
FULLNODE_API_INFO="<token goes here>:/ip4/<host>/tcp/<port>/http
```

Note that if a token is not present in the FULLNODE_API_INFO env variable, the
colon is removed.

Forest developers will prepend this variable to CLI commands over using `export`
on Linux or its equivalent on Windows. This will look like the following:

```
FULLNODE_API_INFO="..." forest-cli auth api-info -p admin
```

The admin token can also be set using `--token` flag.

```
forest-cli --token <ADMIN_TOKEN>
```

### Detaching Forest process

You can detach Forest process via the `--detach` flag so that it runs in the
background:

```bash
./target/release/forest --target-peer-count 50 --detach
```

The command will block until the detached Forest process has started its RPC
server, allowing you to chain some RPC command immediately after.

### Documentation

- [forest book (_Work in progress_)](https://chainsafe.github.io/forest/)
- [rust doc](https://chainsafe.github.io/forest/rustdoc/)

## Contributing

- Check out our contribution guidelines:
  [CONTRIBUTING.md](documentation/src/developer_documentation/contributing.md)

## ChainSafe Security Policy

### Reporting a Security Bug

We take all security issues seriously, if you believe you have found a security
issue within a ChainSafe project please notify us immediately. If an issue is
confirmed, we will take all necessary precautions to ensure a statement and
patch release is made in a timely manner.

Please email a description of the flaw and any related information (e.g.
reproduction steps, version) to
[security at chainsafe dot io](mailto:security@chainsafe.io).

## License

Forest is dual licensed under
[MIT](https://github.com/ChainSafe/forest/blob/main/LICENSE-MIT) +
[Apache 2.0](https://github.com/ChainSafe/forest/blob/main/LICENSE-APACHE).
