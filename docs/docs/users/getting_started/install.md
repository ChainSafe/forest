---
title: Installing
sidebar_position: 2
---

import Tabs from "@theme/Tabs";
import TabItem from "@theme/TabItem";

<Tabs>
  <TabItem value="binaries" label="Binaries" default>

To install Forest from pre-compiled binaries, please refer to the
[releases page](https://github.com/ChainSafe/forest/releases), or consider using
Docker.

<h3> Verifying the installation </h3>

Ensure that Forest was correctly installed.

```shell
forest --version
```

Sample output:

```console
forest-filecoin 0.19.0+git.671c30c
```

  </TabItem>
  <TabItem value="docker" label="Docker">

<h3>Nix Flake</h3>

To install Forest as a Nix flake:

```shell
nix profile install github:ChainSafe/forest
```

This will make the `forest`, `forest-cli`, `forest-tool`, and `forest-wallet`
commands available in your shell.

```shell
forest --version
```

Sample output:

```console
forest-filecoin 0.19.0+git.671c30c
```

<h3>Images</h3>

Images are available via Github Container Registry:

```shell
ghcr.io/chainsafe/forest
```

:::tip
If you have trouble using the Github Container Registry, make sure you are [authenticated with your Github account](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry#authenticating-to-the-container-registry).
:::

You will find tagged images following these conventions:

- `latest` - latest stable release
- `vx.x.x` - tagged versions
- `edge` - latest development build of the `main` branch
- `date-digest` (e.g., `2023-02-17-5f27a62`) - all builds that landed on the `main` branch

A list of available images can be found [here](https://github.com/ChainSafe/forest/pkgs/container/forest).

<h3>Basic Usage</h3>

Running the Forest daemon:

```shell
docker run --init -it --rm ghcr.io/chainsafe/forest:latest --help
```

Using `forest-cli`:

```shell
docker run --init -it --rm --entrypoint forest-cli ghcr.io/chainsafe/forest:latest --help
```

:::note
More information about Docker setup and usage can be found in the [Docker documentation](../knowledge_base/docker_tips.md).
:::

  </TabItem>
  <TabItem value="build" label="Build From Source">

<h3>Dependencies</h3>

- Rust compiler (install via [rustup](https://rustup.rs/))
- OS `Base-Devel`/`Build-Essential`
- Clang compiler
- Go for building F3 sidecar module

For Ubuntu, you can install the dependencies (excluding Rust) with:

```shell
sudo apt install build-essential clang
```

<h3>Compilation & installation</h3>

<h4>Option 1: From crates.io (latest release)</h4>

```shell
cargo install forest-filecoin
```

<h4>Option 2: From repository (latest development branch)</h4>

```shell
git clone --depth 1 https://github.com/ChainSafe/forest.git && cd forest
```

```shell
make install
```

Both approaches will compile and install `forest` and `forest-cli` to
`~/.cargo/bin`. Make sure you have it in your `PATH`.

<h3> Verifying the installation </h3>

Ensure that Forest was correctly installed.

```shell
forest --version
```

Sample output:

```console
forest-filecoin 0.19.0+git.671c30c
```

  </TabItem>
</Tabs>
