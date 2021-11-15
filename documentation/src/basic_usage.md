# Basic Usage

## Build

### Toolchain

- [Rust](https://www.rust-lang.org/tools/install)

### Dependencies

The following commands will install the required dependencies for Forest:

```bash
# Install protoc
PROTOC_ZIP=protoc-3.7.1-linux-x86_64.zip
curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v3.7.1/$PROTOC_ZIP
unzip -o $PROTOC_ZIP -d /usr/local bin/protoc
unzip -o $PROTOC_ZIP -d /usr/local 'include/*'
rm -f $PROTOC_ZIP

# Extra dependencies needed for rust-fil-proofs
apt-get update && \
    apt-get install --no-install-recommends -y curl file gcc g++ hwloc libhwloc-dev git make openssh-client \
    ca-certificates autoconf automake cmake libtool libcurl4 libcurl4-openssl-dev libssl-dev \
    libelf-dev libdw-dev binutils-dev zlib1g-dev libiberty-dev wget \
    xz-utils pkg-config python clang ocl-icd-opencl-dev

# Install binary dependencies
apt-get update && \
    apt-get install --no-install-recommends -y curl file gcc g++ hwloc libhwloc-dev make openssh-client \
    autoconf automake cmake libtool libcurl4 libcurl4-openssl-dev libssl-dev \
    libelf-dev libdw-dev binutils-dev zlib1g-dev libiberty-dev wget \
    xz-utils pkg-config python clang ocl-icd-opencl-dev ca-certificates
```

### Commands

```bash
make release
```

## Forest Import Snapshot Mode

Before running `forest` in the normal mode you must seed the database with the Filecoin state tree from the latest snapshot. To do that, we will download the latest snapshot provided by Protocol Labs and start `forest` using the `--import-snapshot` flag. After the snapshot has been successfully imported, you can start `forest` without the `--import-snapshot` flag.

### Commands

Download the latest snapshot provided by Protocol Labs:

```bash
wget https://fil-chain-snapshots-fallback.s3.amazonaws.com/mainnet/minimal_finality_stateroots_latest.car > /destination/for/snapshot/file
```

Import the snapshot using `forest`:

```bash
./target/release/forest --target-peer-count 50 --encrypt-keystore false --import-snapshot /path/to/snapshot/file
```

## Forest Synchronization Mode

### Commands

#### Mainnet

Start the `forest` node:

```bash
./target/release/forest --target-peer-count 50 --encrypt-keystore false
```
