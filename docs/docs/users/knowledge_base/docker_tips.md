---
title: Docker Tips & Tricks
sidebar_position: 3
---

# Forest in Docker🌲❤️🐋

## Prerequisites

- Docker engine [installed](https://docs.docker.com/get-started/get-docker/) and running. Forest containers are confirmed to run on
  the following engines:
  - Docker Engine (Community) on Linux,
  - Docker for macOS
  - Docker on Windows Subsystem for Linux 2(WSL2)

Native images are available for the following platform/architecture(s):

- `linux/arm64`
- `linux/amd64`

The images will work out-of-the box on both Intel processors and macOS with
M1 / M2.

## Tags

For the list of all available tags please refer to the
[Forest packages](https://github.com/ChainSafe/forest/pkgs/container/forest).

Currently, the following tags are produced:

- `latest` - latest stable release,
- `latest-fat` - latest stable release with necessary downloadable files preloaded,
- `edge` - latest development build of the `main` branch,
- `edge-fat` - latest development build of the `main` branch with necessary downloadable files preloaded,
- `date-digest` e.g., `2023-02-17-5f27a62` - all builds that landed on the
  `main` branch,
- `date-digest-fat` e.g., `2023-02-17-5f27a62-fat` - all builds that landed on the
  `main` branch with necessary downloadable files preloaded,
- release tags, available from `v0.7.0` or `v0.7.0-fat` onwards.

## Security recommendations

- We strongly recommend running the docker daemon in rootless mode
  ([installation instructions](https://docs.docker.com/engine/security/rootless/)),
  or running the daemon-less docker alternative `podman`
  ([installation instructions](https://podman.io/getting-started/installation))
  with non-root user and put `alias docker = podman` (or manually replace the
  `docker` commands with `podman` in below instructions)

## Performance recommendations

- We recommend lowering the swappiness kernel parameter on Linux to 1-10 for
  long running forest node by doing `sudo sysctl -w vm.swappiness=[n]`.

References: [1](https://en.wikipedia.org/wiki/Memory_paging#Swappiness)
[2](https://linuxhint.com/understanding_vm_swappiness/)

## Example usages

### List available flags and/or commands

```shell
# daemon
docker run --init -it --rm ghcr.io/chainsafe/forest:latest --help
# cli
docker run --init -it --rm --entrypoint forest-cli ghcr.io/chainsafe/forest:latest --help
# tool
docker run --init -it --rm --entrypoint forest-tool ghcr.io/chainsafe/forest:latest --help
# wallet tool
docker run --init -it --rm --entrypoint forest-wallet ghcr.io/chainsafe/forest:latest --help
```

Also see the [CLI documentation](../reference/cli) for more details about commands and
their usage.

### Run a Forest node with custom environment variables

```shell
docker run --init -it --rm --name forest --env <key>=<value> ghcr.io/chainsafe/forest:latest --chain calibnet --auto-download-snapshot
```

Check [Forest environment variables documentation](../reference/env_variables) for more details.

### Create a Forest node running calibration network. Then list all connected peers.

Interactive mode:

```shell
docker run --init -it --rm --name forest ghcr.io/chainsafe/forest:latest --chain calibnet --auto-download-snapshot
```

Non-interactive mode

```
docker run --init --name forest ghcr.io/chainsafe/forest:latest --chain calibnet --auto-download-snapshot
```

:::tip
[watchtower](https://github.com/containrrr/watchtower) is a great tool for keeping the forest image up-to-date, automagically, check [instructions](https://containrrr.dev/watchtower/#quick_start).
:::

Then, in another terminal:

```shell
docker exec forest forest-cli net peers
```

Sample output:

```console
12D3KooWAh4qiT3ZRZgctVJ8AWwRva9AncjMRVBSkFwNjTx3EpEr, [/ip4/10.0.2.215/tcp/1347, /ip4/52.12.185.166/tcp/1347]
12D3KooWMY4VdMsdbFwkHv9HxX2jZsUdCcWFX5F5VGzBPZkdxyVr, [/ip4/162.219.87.149/tcp/30141, /ip4/162.219.87.149/tcp/30141/p2p/12D3KooWMY4VdMsdbFwkHv9HxX2jZsUdCcWFX5F5VGzBPZkdxyVr]
12D3KooWFWUqE9jgXvcKHWieYs9nhyp6NF4ftwLGAHm4sCv73jjK, [/dns4/bootstrap-3.calibration.fildev.network/tcp/1347]
```

### Use a shared volume to utilize across different Forest images

Create the volume

```shell
docker volume create forest-data
```

Now, whenever you create a new Forest container, attach the volume to where the
data is stored `/root/.local/share/forest`.

```shell
docker run --init -it --rm \
           --ulimit nofile=8192 \
           --volume forest-data:/root/.local/share/forest \
           --name forest ghcr.io/chainsafe/forest:latest --chain calibnet
                                                         --auto-download-snapshot
```

### Export the calibnet snapshot to the host machine

Assuming you have `forest` container already running, run:

```shell
docker exec forest forest-cli --chain calibnet snapshot export
```

Sample output:

```console
Export completed. Snapshot located at forest_snapshot_calibnet_2023-02-17_height_308891.car
```

Copy the snapshot to the host

```shell
docker cp forest:/home/forest/forest_snapshot_calibnet_2023-02-17_height_308891.car .
```
