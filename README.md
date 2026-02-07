<p align="center">
  <img height="243" src="docs/static/img/forest_logo.png">
</p>

<p align="center">
    <a href="https://github.com/ChainSafe/forest/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/ChainSafe/forest?style=for-the-badge"></a>
    <a href="https://docs.forest.chainsafe.io"><img alt="Docs" src="https://img.shields.io/badge/doc-user_guide-green?style=for-the-badge"></a>
    <a href="https://codecov.io/github/ChainSafe/forest"><img alt="Codecov" src="https://codecov.io/github/ChainSafe/forest/graph/badge.svg?token=1OHO2CSD17"/></a>
</p>
 <p align="center">
    <a href="https://github.com/ChainSafe/forest/blob/main/LICENSE-APACHE"><img alt="License Apache 2.0" src="https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge"></a>
    <a href="https://github.com/ChainSafe/forest/blob/main/LICENSE-MIT"><img alt="License MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge"></a>
    <a href="https://twitter.com/ChainSafeth"><img alt="Twitter" src="https://img.shields.io/twitter/follow/chainsafeth?style=for-the-badge&color=1DA1F2"></a>
</p>

Forest is a [Filecoin] node written in [Rust]. With Forest, you can:

- Transfer FIL,
- host a high-performance RPC API,
- validate the Filecoin blockchain,
- generate blockchain snapshots.

While less feature-complete than the canonical Filecoin node, [Lotus], Forest
aims to be the faster and easier-to-use alternative.

## Questions

Have questions? Feel free to post them in [Forest Q&A]!

## Run with Docker

No need to install Rust toolchain or other dependencies, you will need only
Docker - works on Linux, macOS and Windows.

```
# daemon
❯ docker run --init -it --rm ghcr.io/chainsafe/forest:latest --help
# cli
❯ docker run --init -it --rm --entrypoint forest-cli ghcr.io/chainsafe/forest:latest --help
```

Next, run a Forest node in a CLI window. E.g.
[Run calibration network](https://docs.forest.chainsafe.io/getting_started/syncing/#calibnet)

Thereafter, in another terminal, you will be able to use the `forest-cli` binary
directly by launching `bash` in the `forest` container:

```
docker exec -it forest /bin/bash
```

For more in-depth usage and sample use cases, please refer to the Forest Docker
documentation in the [Forest Book]. Keep in mind that the `latest` tag is the latest
stable release. If you want to use the current development build, use the `edge`
tag.

## Dependencies

- Rust (toolchain version is specified in `rust-toolchain.toml`)
- Go for building F3 sidecar module. (toolchain version is specified in
  `go.work`)

Install [rustup](https://rustup.rs/)

Install [Go](https://go.dev/doc/install)

- OS Base-Devel/Build-Essential
- Clang compiler

The project also uses [mise-en-place](https://mise.jdx.dev/) to handle builds and
installations.

### Ubuntu (20.04)

```
sudo apt install build-essential clang
```

### Fedora (36)

```
sudo dnf install -y clang-devel
```

## Installation

```shell
# Clone repository
git clone --recursive https://github.com/chainsafe/forest
cd forest

# Install binary to $HOME/.cargo/bin
mise run install

# Run the node on mainnet
forest
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
[client]
data_dir = "<directory for all chain and networking data>"
genesis_file = "<relative file path of genesis car file>"

[network]
listening_multiaddrs = ["<multiaddress>"]
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
# To run all tests
mise test
# Or, with a different profile
mise test release
```

Chain synchronization checks are run after every merge to `main`. This code is
maintained in a separate repository - [Forest IaC].

### Linters

The project uses exhaustively a set of linters to keep the codebase clean and
secure in an automated fashion. While the CI will have them installed, if you
want to run them yourself before submitting a PR (recommended), you should
install a few of them.

You can install required linters with `mise install-lint-tools`.
After everything is installed, you can run `mise lint`.

### Joining the testnet

Select the builtin calibnet configuration with the `--chain` option. The
`--auto-download-snapshot` will ensure that a snapshot is downloaded if needed
without any prompts.

```bash
./target/release/forest --chain calibnet --auto-download-snapshot
```

### Interacting with Forest via CLI

When the Forest daemon is started, an admin token will be displayed and saved to
data directory by default. (alternatively, use `--save-token <token>` flag to save it on disk).
You will need this for commands that require a higher level of authorization (like a
password). Forest, as mentioned above, uses multiaddresses for networking. This
is no different in the CLI. To set the host and the port to use, if not using
the default port or using a remote host, set the `FULLNODE_API_INFO` environment
variable. This is also where you can set a token for authentication. Note that the token is
automatically set for CLI if it is invoked on the same host of the daemon.

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

### Forest executable organization

The binaries in the Forest repository are organized into the following
categories:

| Binary                                                                          | Role                                                     | Command example                                    |
| ------------------------------------------------------------------------------- | -------------------------------------------------------- | -------------------------------------------------- |
| [`forest`](https://docs.forest.chainsafe.io/reference/cli#forest)               | Forest daemon, used to connect to the Filecoin network   | `forest --chain calibnet --auto-download-snapshot` |
| [`forest-wallet`](https://docs.forest.chainsafe.io/reference/cli#forest-wallet) | Manage Filecoin wallets and interact with accounts       | `forest-wallet new secp256k1`                      |
| [`forest-cli`](https://docs.forest.chainsafe.io/reference/cli#forest-cli)       | Human-friendly wrappers around the Filecoin JSON-RPC API | `forest-cli info show`                             |
| [`forest-tool`](https://docs.forest.chainsafe.io/reference/cli#forest-tool)     | Handle tasks not involving the Forest daemon             | `forest-tool snapshot fetch`                       |

### Detaching Forest process

You can detach Forest process with `nohup` or `disown` from the terminal so that it runs in the
background after the terminal is closed:

```bash
nohup ./target/release/forest &
```

```bash
./target/release/forest &
disown
```

The command will block until the detached Forest process has started its RPC
server, allowing you to chain some RPC command immediately after.

### Forest snapshot links

- [calibration network](https://forest-archive.chainsafe.dev/latest/calibnet/)
- [main network](https://forest-archive.chainsafe.dev/latest/mainnet/)

### Documentation

- [Forest Book](https://docs.forest.chainsafe.io/)

## Contributing

- Check out our contribution guidelines: [CONTRIBUTING.md]

## ChainSafe Security Policy

### Reporting a Security Bug

We take all security issues seriously, if you believe you have found a security
issue within a ChainSafe project please notify us immediately. If an issue is
confirmed, we will take all necessary precautions to ensure a statement and
patch release is made in a timely manner.

Please email a description of the flaw and any related information (e.g.
reproduction steps, version) to [security at chainsafe dot io].

## License

Forest is dual licensed under [MIT] + [Apache 2.0].

[Filecoin]: https://filecoin.io/
[Rust]: https://www.rust-lang.org/
[Lotus]: https://lotus.filecoin.io/
[Forest Book]: https://docs.forest.chainsafe.io/
[Forest Q&A]: https://github.com/ChainSafe/forest/discussions/categories/forest-q-a
[CONTRIBUTING.md]: CONTRIBUTING.md
[Forest IaC]: https://github.com/ChainSafe/forest-iac
[security at chainsafe dot io]: mailto:security@chainsafe.io
[MIT]: https://github.com/ChainSafe/forest/blob/main/LICENSE-MIT
[Apache 2.0]: https://github.com/ChainSafe/forest/blob/main/LICENSE-APACHE
