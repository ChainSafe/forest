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

Use [mise-en-place](https://mise.jdx.dev/) to handle the build and installation:

```shell
mise install
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
  <TabItem value="systemd" label="Systemd Unit Setup">

<h3>Running Forest as a `systemd` Service</h3>

This guide shows how to configure Forest to automatically restart on failure and start on system boot using `systemd`.

<h4>Prerequisites</h4>

- Forest must be installed and available in your `PATH` (see other tabs for installation). This guide assumes the `forest` binary is located at `/usr/local/bin/forest`.
- You are running commands as `root` or with `sudo` privileges
- `vi` editor. If you're using `nano`, reconsider your life choices and career path.

<h4>Step 1: Create a `systemd` Service File</h4>

Create a new service file for Forest:

```shell
vi /etc/systemd/system/forest.service
```

Add the following content (adjust paths and options as needed):

```ini
[Unit]
Description=Forest Filecoin Node
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=forest
Group=forest
# Adjust the forest binary path if needed (check with: which forest)
# You might want to encrypt the keystore in production with `--encrypt-keystore true` and using, e.g., `systemd-creds`
ExecStart=/usr/local/bin/forest --chain calibnet --auto-download-snapshot --encrypt-keystore false --rpc-address=127.0.0.1:1234
# Or for mainnet:
# ExecStart=/usr/local/bin/forest --encrypt-keystore false

# Restart policy
Restart=on-failure
RestartSec=10s

Environment=FOREST_CHAIN_INDEXER_ENABLED=1
# Optional, if F3 is not working properly.
# Environment=FOREST_F3_SIDECAR_FFI_ENABLED=0

[Install]
WantedBy=multi-user.target
```

:::tip
For production mainnet nodes, consider creating a dedicated `forest` user for better security isolation. For development/testing, you can use your own user.
:::

<h4>Step 2: Create a Dedicated User (Optional but Recommended)</h4>

If you specified `User=forest` in the service file, create the user:

```shell
adduser forest
```

Make sure the binary is accessible by this user.

<h4>Step 3: Enable and Start the Service</h4>

Reload `systemd` to recognize the new service:

```shell
systemctl daemon-reload
```

Enable the service to start on boot:

```shell
systemctl enable forest
```

Start the service immediately:

```shell
systemctl start forest
```

<h4>Step 4: Verify the Service is Running</h4>

Check the service status:

```shell
systemctl status forest
```

Sample output:

```console
● forest.service - Forest Filecoin Node
     Loaded: loaded (/etc/systemd/system/forest.service; enabled; vendor preset: enabled)
     Active: active (running) since Wed 2026-01-28 10:30:15 UTC; 2min ago
   Main PID: 12345 (forest)
```

<h4>Step 5: View Logs</h4>

View real-time logs:

```shell
journalctl -u forest -f
```

View recent logs:

```shell
journalctl -u forest -n 100
```

<h4>Troubleshooting</h4>

If the service fails to start:

1. Check logs with `journalctl -u forest -n 50`
2. Verify the `forest` binary path with `which forest`
3. Ensure the user has appropriate permissions
4. Check that required directories exist and are writable

:::note
The `Restart=on-failure` option ensures Forest automatically restarts if it crashes. The `RestartSec=10s` adds a 10-second delay between restart attempts to prevent rapid restart loops.
:::

  </TabItem>
</Tabs>
