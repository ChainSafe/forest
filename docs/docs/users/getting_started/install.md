---
title: Installing
sidebar_position: 2
---

import Tabs from "@theme/Tabs";
import TabItem from "@theme/TabItem";

<Tabs>
  <TabItem value="binaries" label="Binaries" default>

To install Forest from pre-compiled binaries, please refer to the
[releases page](https://GitHub.com/ChainSafe/forest/releases), or consider using
Docker.

  </TabItem>
  <TabItem value="docker" label="Docker">

<h3>Images</h3>

Images are available via GitHub Container Registry:

```shell
ghcr.io/chainsafe/forest
```

You will find tagged images following these conventions:

- `latest` - latest stable release
- `vx.x.x` - tagged versions
- `edge` - latest development build of the `main` branch
- `date-digest` (e.g., `2023-02-17-5f27a62`) - all builds that landed on the `main` branch

A list of available images can be found here: https://GitHub.com/ChainSafe/forest/pkgs/container/forest

<h3>Basic Usage</h3>

Running the Forest daemon:

```shell
❯ docker run --init -it --rm ghcr.io/chainsafe/forest:latest --help
```

Using `forest-cli`:

```shell
❯ docker run --init -it --rm --entrypoint forest-cli ghcr.io/chainsafe/forest:latest --help
```

  </TabItem>
  <TabItem value="build" label="Build From Source">

<h3>Dependencies</h3>

- Rust compiler (install via [rustup](https://rustup.rs/))
- OS `Base-Devel`/`Build-Essential`
- Clang compiler

For Ubuntu, you can install the dependencies (excluding Rust) with:

```shell
sudo apt install build-essential clang
```

<h3>Compilation & installation</h3>

<h4>From crates.io (latest release)</h4>

```shell
cargo install forest-filecoin
```

<h4>From repository (latest development branch)</h4>

```shell
# Clone the Forest repository
git clone --depth 1 https://GitHub.com/ChainSafe/forest.git && cd forest
make install
```

Both approaches will compile and install `forest` and `forest-cli` to
`~/.cargo/bin`. Make sure you have it in your `PATH`.

  </TabItem>
</Tabs>

## Verifying the installation

Ensure that Forest was correctly installed.

```shell
❯ forest --version
forest-filecoin 0.19.0+git.671c30c
```
