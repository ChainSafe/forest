# Forest in Docker🌲❤️🐋

## Prerequisites

- Docker engine installed and running. Forest containers are confirmed to run on
  the following engines:
  - Docker Engine (Community) on Linux,
  - Docker for macOS
  - Podman on WSL

Native images are available for the following platforms:

- `linux/arm64`
- `linux/amd64`

The images will work out-of-the box on both Intel processors and macOS with
M1/M2.

## Tags

For the list of all available tags please refer to the
[Forest packages](https://github.com/ChainSafe/forest/pkgs/container/forest).

Currently, the following tags are produced:

- `latest` - latest stable release,
- `edge` - latest development build of the `main` branch,
- `date-digest` e.g., `2023-02-17-5f27a62` - all builds that landed on the
  `main` branch,
- release tags, available from `v.0.7.0` onwards.

## Security recommendations

- We strongly recommend running the docker daemon in rootless mode
  ([installation instructions](https://docs.docker.com/engine/security/rootless/)),
  or running the daemon-less docker alternative `podman`
  ([installation instructions](https://podman.io/getting-started/installation))
  with non-root user and put `alias docker = podman` (or manually replace the
  `docker` commands with `podman` in below instructions)

## Performance recommendations

- We recommend lowering the swappiness kernel parameter on linux to 1-10 for
  long running forest node by doing `sudo sysctl -w vm.swappiness=[n]`.

References: [1](https://en.wikipedia.org/wiki/Memory_paging#Swappiness)
[2](https://linuxhint.com/understanding_vm_swappiness/)

## Usage

### List available flags and/or commands

```shell
# daemon
❯ docker run --init -it --rm ghcr.io/chainsafe/forest:latest --help
# cli
❯ docker run --init -it --rm --entrypoint forest-cli ghcr.io/chainsafe/forest:latest --help
```

### Create a Forest node running calibration network. Then list all connected peers.

```shell
❯ docker run --init -it --rm --name forest ghcr.io/chainsafe/forest:latest --chain calibnet --auto-download-snapshot
```

then in another terminal (sample output)

```shell
❯ docker exec -it forest forest-cli net peers
12D3KooWAh4qiT3ZRZgctVJ8AWwRva9AncjMRVBSkFwNjTx3EpEr, [/ip4/10.0.2.215/tcp/1347, /ip4/52.12.185.166/tcp/1347]
12D3KooWMY4VdMsdbFwkHv9HxX2jZsUdCcWFX5F5VGzBPZkdxyVr, [/ip4/162.219.87.149/tcp/30141, /ip4/162.219.87.149/tcp/30141/p2p/12D3KooWMY4VdMsdbFwkHv9HxX2jZsUdCcWFX5F5VGzBPZkdxyVr]
12D3KooWFWUqE9jgXvcKHWieYs9nhyp6NF4ftwLGAHm4sCv73jjK, [/dns4/bootstrap-3.calibration.fildev.network/tcp/1347]
```

### Use a shared volume to utilise across different Forest images

Create the volume

```shell
docker volume create forest-data
```

Now, whenever you create a new Forest container, attach the volume to where the
data is stored `/home/forest/.local/share/forest`.

```shell
❯ docker run --init -it --rm \
             --ulimit nofile=8192 \
             --volume forest-data:/home/forest/.local/share/forest \
             --name forest ghcr.io/chainsafe/forest:latest --chain calibnet
                                                           --auto-download-snapshot
```

### Export the calibnet snapshot to the host machine

Assuming you have `forest` container already running, run:

```shell
❯ docker exec -it forest forest-cli --chain calibnet snapshot export
Export completed. Snapshot located at forest_snapshot_calibnet_2023-02-17_height_308891.car
```

Copy the snapshot to the host

```shell
❯ docker cp forest:/home/forest/forest_snapshot_calibnet_2023-02-17_height_308891.car .
```

### Create and fund a wallet, then send some FIL on calibration network

Assuming you have `forest` container already running, you need to find the JWT
token in the logs.

```shell
❯ docker logs forest | grep "Admin token"
```

export it to an environmental variable for convenience (sample, use the token
you obtained in the previous step)

```shell
export JWT_TOKEN=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXSwiZXhwIjoxNjgxODIxMTc4fQ.3toXEeiGcHT01pUjQeqMyW2kZmQpqpE4Gi4vOHjX4rE
```

Create the wallet

```shell
❯ docker exec -it forest forest-cli --chain calibnet --token $JWT_TOKEN wallet new
t1uvqpa2jgic7fhhko3w4wf3kxj36qslvqrk2ln5i
```

You can fund your wallet using this
[faucet](https://faucet.calibration.fildev.network/funds.html). If this faucet
is unavailable or does not work, there is an
[alternative faucet](https://faucet.triangleplatform.com/filecoin/calibration).
You can verify your wallet was funded after a few minutes in
[Filscan](https://calibration.filscan.io/) by pasting the Message ID obtained
from the faucet. Example from
[this wallet](https://calibration.filscan.io/tipset/message-detail?cid=bafy2bzacebdverplts5qs3lwzsenzlh4rdsmvc42r6yg6suu4comr7gkbe76a).

Verify that your account has 100 FIL . The result is in `attoFIL`.

```shell
❯ docker exec -it forest forest-cli --chain calibnet --token $JWT_TOKEN wallet balance t1uvqpa2jgic7fhhko3w4wf3kxj36qslvqrk2ln5i
100000000000000000000
```

Create another wallet

```shell
❯ docker exec -it forest forest-cli --chain calibnet --token $JWT_TOKEN wallet new
t1wa7lgs7b3p5a26abkgpxwjpw67tx4fbsryg6tca
```

Send 10 FIL from the original wallet to the new one

```shell
❯ docker exec -it forest forest-cli --chain calibnet --token $JWT_TOKEN send --from t1uvqpa2jgic7fhhko3w4wf3kxj36qslvqrk2ln5i t1wa7lgs7b3p5a26abkgpxwjpw67tx4fbsryg6tca 10000000000000000000
```

Verify the balance of the new address.
[Sample transaction](https://calibration.filscan.io/tipset/message-detail?cid=bafy2bzacebymw25tedmec4xnwmf7fcrt64qvfbbuacbx6lnhyrcbfv3rgkn2a)
for this wallet.

```shell
❯ docker exec -it forest forest-cli --chain calibnet --token $JWT_TOKEN wallet balance t1wa7lgs7b3p5a26abkgpxwjpw67tx4fbsryg6tca
10000000000000000000
```
