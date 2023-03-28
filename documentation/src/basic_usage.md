# Basic Usage

## Build

### Dependencies

- Rust `rustc >= 1.58.1`
- Rust WASM target `wasm32-unknown-unknown`

```shell
rustup install stable
rustup target add wasm32-unknown-unknown
```

- OS Base-Devel/Build-Essential
- Clang compiler
- OpenCL bindings

```shell
# Ubuntu
sudo apt install build-essential clang

# Archlinux
sudo pacman -S base-devel clang
```

### Commands

```bash
make release
```

## Forest Import Snapshot Mode

Before running `forest` in the normal mode you must seed the database with the
Filecoin state tree from the latest snapshot. To do that, we will download the
latest snapshot provided by Protocol Labs and start `forest` using the
`--import-snapshot` flag. After the snapshot has been successfully imported, you
can start `forest` without the `--import-snapshot` flag.

### Commands

Download the latest snapshot provided by Protocol Labs:

```bash
curl -sI https://fil-chain-snapshots-fallback.s3.amazonaws.com/mainnet/minimal_finality_stateroots_latest.car | perl -ne '/x-amz-website-redirect-location:\s(.+)\.car/ && print "$1.sha256sum\n$1.car"' | xargs wget
```

If desired, you can check the checksum using the instructions
[here](https://lotus.filecoin.io/docs/set-up/chain-management/#lightweight-snapshot).

Import the snapshot using `forest`:

```bash
forest --target-peer-count 50 --encrypt-keystore false --import-snapshot /path/to/snapshot/file
```

## Forest Synchronization Mode

### Commands

#### Mainnet

Start the `forest` node:

```bash
forest --target-peer-count 50 --encrypt-keystore false
```
