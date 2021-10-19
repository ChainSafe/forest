# Basic Usage

## Build

### Toolchain

- [Rust](https://www.rust-lang.org/tools/install) 

### Commands

```bash
make release
```

## Run Forest

### Commands

#### Mainnet

Start the `forest` node:

```bash
./target/release/forest --target-peer-count 50 --encrypt-keystore false
```

## Import Snapshot

### Commands

Download the latest snapshot provided by Protocol Labs:

```bash
wget https://fil-chain-snapshots-fallback.s3.amazonaws.com/mainnet/minimal_finality_stateroots_latest.car > /destination/for/snapshot/file
```

Import the snapshot using `forest`:

```bash
./target/release/forest --target-peer-count 50 --encrypt-keystore false --import-snapshot /path/to/snapshot/file
```