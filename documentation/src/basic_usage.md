# Basic Usage

## Installation with pre-built binaries

To install Forest from pre-compiled binaries, please refer to the
[releases page](https://github.com/ChainSafe/forest/releases) or consider using
Forest Docker image (explained in detail [here](docker.md)).

## Installation from source

### Dependencies

- Rust - install via [rustup](https://rustup.rs/)
- OS Base-Devel/Build-Essential
- Clang compiler
- OpenCL bindings

For Ubuntu, you can install the dependencies (excluding Rust) with:

```shell
sudo apt install build-essential clang
```

### Optional runtime dependencies

[aria2](https://aria2.github.io/) is an alternate backend for downloading the
snapshots. It is significantly faster than the in-built Forest downloader.

```shell
sudo apt install aria2
```

### Compilation & installation

#### From crates.io (latest release)

```shell
cargo install forest-filecoin
```

#### From repository (latest development branch)

```shell
# Clone the Forest repository
git clone --depth 1 https://github.com/ChainSafe/forest.git && cd forest
make install
```

Both approaches will compile and install `forest` and `forest-cli` to
`~/.cargo/bin`. Make sure you have it in your `PATH`.

## Verifying the installation

Ensure that Forest was correctly installed.

```shell
forest --version
# forest-filecoin 0.10.0+git.2eaeb9fee
```

## Synchronize to the Filecoin network

Start the `forest` node. It will automatically connect to the bootstrap peers
and start syncing the chain after the snapshot is downloaded. If it is your
first time running the node, it will take a while to download the snapshot. Note
that you will need at least 8GB of RAM to sync the mainnet chain, and over 100
GB of free disk space.

#### Mainnet

```shell
forest
```

#### Calibnet

```shell
forest --chain calibnet
```

In another shell, you can invoke commands on the running node using
`forest-cli`. For example, to check the synchronization status:

```shell
forest-cli sync status
```
