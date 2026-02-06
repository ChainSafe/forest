# AGENTS.md

This file provides guidance to AI coding assistants (such as Claude Code, Cursor, Copilot, etc.) when working with code in this repository.

## Project Overview

Forest is a Rust implementation of a Filecoin node that can transfer FIL, host a high-performance RPC API, validate the Filecoin blockchain, and generate blockchain snapshots. It aims to be faster and easier-to-use than the canonical Filecoin node (Lotus).

## Development Commands

### Building and Installing

```bash
# Install Forest binaries with release profile (recommended)
mise run install

# Install with different profiles
mise run install quick          # Faster build, less optimization
mise run install release-lto-fat # Maximum optimization (slower build)
mise run install dev            # Debug build

# Install slim version (minimal features)
mise run install --slim

# Build without installing
cargo build --release
cargo build --profile quick  # Faster compile time

# Run binaries directly (for development)
cargo daemon --chain calibnet  # Alias for: cargo run --bin forest --
cargo cli info show            # Alias for: cargo run --bin forest-cli --
cargo forest-tool --help       # Alias for: cargo run --bin forest-tool --release --
```

### Testing

```bash
# Run all tests (requires cargo-nextest: cargo install cargo-nextest --locked)
mise test           # Uses 'quick' profile by default
mise test release   # Run with release profile
mise test dev       # Run with dev profile

# Run only Rust tests (no doctests)
mise test:rust
mise test:rust release

# Run only doctests
mise test:docs
mise test:docs release

# Run specific test
cargo nextest run --cargo-profile quick <test_name>

# Run tests in a specific file/module
cargo nextest run --cargo-profile quick state_manager::

# Run single test with full output
cargo nextest run --cargo-profile quick --no-capture <test_name>

# Run doctests for private items
cargo test --doc --profile quick --features doctest-private
```

### Linting and Formatting

```bash
# Install all linting tools
mise install-lint-tools

# Run all linters
mise lint

# Run specific linters
mise lint:rust-fmt    # Rust formatting check
mise lint:clippy      # Rust linter
mise lint:toml        # TOML formatting/linting
mise lint:spellcheck  # Spell checking
mise lint:deny        # Check licenses and security
mise lint:unused-deps # Check for unused dependencies
mise lint:dockerfile  # Lint Dockerfiles
mise lint:shellcheck  # Lint shell scripts
mise lint:golang      # Lint Go code (F3 sidecar)

# Format code
mise fmt              # Format Rust, TOML, markdown, YAML

# Check specific issues
cargo fmt --all -- --check
cargo clippy --all-targets --no-deps -- --deny=warnings
taplo fmt --check && taplo lint
```

### Code Coverage

```bash
# Generate coverage report (requires cargo-llvm-cov: cargo install cargo-llvm-cov)
mise codecov
```

### Cleaning

```bash
# Clean all build artifacts and dependencies
mise clean
```

## High-Level Architecture

### Core Modules

- **`daemon/`** - Node startup, initialization, and service orchestration
- **`chain/`** - Blockchain storage (`ChainStore`) and index management
- **`chain_sync/`** - Chain synchronization, consensus (`ChainFollower`, `ChainMuxer`)
- **`state_manager/`** - State tree management and FVM execution coordinator
- **`rpc/`** - JSON-RPC API server with authentication and filtering middleware
- **`libp2p/`** - P2P networking (peer discovery, chain exchange, gossipsub)
- **`message_pool/`** - Transaction pool for pending messages
- **`db/`** - Database abstraction layer (ParityDb, MemoryDB, CAR format)
- **`interpreter/`** - Filecoin Virtual Machine (FVM) integration (multi-version)
- **`blocks/`** - Block and tipset structures
- **`shim/`** - Filecoin protocol primitives (actors, crypto, addresses, state tree)
- **`eth/`** - Ethereum compatibility layer (EVM transactions, address mapping)
- **`wallet/`** - Key management and transaction signing
- **`networks/`** - Network configurations (mainnet, calibnet, devnet)

### Key Architectural Patterns

**Blockstore Pattern**: Generic database trait (`fvm_ipld_blockstore::Blockstore`) allows swapping storage implementations. Most core structures are generic over `DB: Blockstore`:

```rust
pub struct StateManager<DB> where DB: Blockstore { ... }
pub struct ChainStore<DB> where DB: Blockstore { ... }
```

**Publisher/Subscriber**: `HeadChange` events (new tipsets, reorg reverts) are broadcast via `Publisher` to multiple subscribers (RPC, message pool, chain indexer).

**State Management**: `StateManager` is the central coordinator for state transitions, actor queries, and FVM execution. It caches state roots and receipts in `TipsetStateCache`.

**Multi-version FVM**: Supports FVM2, FVM3, and FVM4 for different network versions. Stack size management with `stacker::grow()` is required for WASM execution.

### Daemon Startup Flow

1. `startup_init()` - Increase file descriptor limits, initialize proof cache
2. `AppContext::init()` - Initialize database, state manager, keystore, JWT
3. `create_p2p_service()` - Start libp2p networking stack
4. `create_mpool()` - Initialize message pool
5. `create_chain_follower()` - Start chain synchronization
6. `maybe_start_rpc_service()` - Start JSON-RPC server
7. `maybe_start_metrics_service()` - Start Prometheus metrics endpoint
8. `maybe_start_health_check_service()` - Start health check service
9. `maybe_start_f3_service()` - Start F3 sidecar (Fast Finality, optional)

### Chain Synchronization

**ChainFollower** orchestrates synchronization:

- Receives tipsets from network peers
- Validates blocks via `TipsetSyncer` and `TipsetValidator`
- Resolves forks via `ChainMuxer` (heaviest chain wins)
- Updates chain head through `StateManager`
- Maintains `BadBlockCache` for invalid blocks
- Reports `SyncStatus` to RPC clients

### RPC Server Architecture

Multi-layer middleware stack:

```
Request → AuthLayer → FilterLayer → SegregationLayer →
SetExtensionLayer → LogLayer → MetricsLayer → RpcHandler
```

Major RPC namespaces: `auth::`, `chain::`, `state::`, `mpool::`, `eth::`, `net::`, `sync::`, `wallet::`

### FVM Integration

**Version Selection**: Network version determines FVM version (FVM2 for v1-v15, FVM3 for v16-v20, FVM4 for v21+)

**VM Execution Flow**:

```
StateManager::call_with_gas()
  → VM::new(ExecutionContext)
  → vm.apply_message() / apply_implicit_message()
  → VM::flush() → new_state_root
```

**Key Concepts**:

- **State Tree** - IPLD-based Merkle tree of actor states
- **Actors** - Smart contracts (System, Init, Power, Market, Miner, etc.)
- **Messages** - Transactions that execute actor methods
- **Receipts** - Execution results (gas used, exit code, return value)
- **Randomness** - Provided by Drand beacon (via `ChainRand`)
- **State Migrations** - Automatic upgrades at network version boundaries

### P2P Networking

Libp2p protocols:

- **Chain Exchange** - Fetch blocks and messages during sync
- **Hello** - Exchange peer info and genesis CID
- **Gossipsub** - Broadcast new blocks and messages
- **Bitswap** - IPLD block exchange (legacy)
- **Kademlia DHT** - Peer discovery
- **mDNS** - Local network discovery

**Peer Manager** tracks peer quality, manages connections, and scores peers based on message validity.

### Database Organization

Storage layers:

```
Application (StateManager, ChainStore)
  ↓
IPLD Blockstore (LogicalDB)
  ↓
Write Buffer / Read Cache (optional)
  ↓
ParityDb (embedded KV store)
```

Special stores:

- **Settings Store** - Configuration persistence (chain head, message pool config)
- **Eth Mappings Store** - ETH ↔ Filecoin address mappings
- **Indices Store** - Message and event indices
- **CAR DB** - Snapshot import/export (v1, v2 with F3 data)

### Ethereum Compatibility

Supports legacy transactions, EIP-155, and EIP-1559. Provides standard Ethereum RPC methods (`eth_call`, `eth_sendTransaction`, `eth_getTransactionReceipt`, `eth_subscribe`, etc.)

**Address Mapping**: EVM addresses map to Filecoin f4 (delegated) addresses. Precompiles provide Filecoin-specific operations.

## Project-Specific Patterns

### Error Handling

Use `anyhow::Result<T>` for most operations. Add context with `.context()`:

```rust
some_operation().context("Failed to execute VM")?
```

### Async/Await

- Tokio runtime for async execution
- Use `tokio::task::spawn_blocking` for CPU-intensive work (VM execution, cryptography)
- Channel-based communication between tasks (flume, tokio channels)

### Module Organization

Each module has:

- `mod.rs` - Public API exports
- Private submodules for implementation details
- Clear trait boundaries for extensibility

### Code Style

- **No indexing** - Use `.get()` instead of `[index]` (enforced by clippy)
- **No unwrap in production** - Use `?` or `expect()` with descriptive messages
- **No dbg! or todo!** - Enforced in non-test code
- Use `strum` for enum string conversions
- Use `derive_more` for common trait implementations

### Testing Utilities

- `test_utils/` provides fixtures and helpers
- Use `#[cfg(test)]` or `#[cfg(feature = "doctest-private")]` for test-only code
- Serial tests (database tests) use `#[serial_test::serial]`

### Network-Specific Configuration

Networks defined in `networks/`:

- **mainnet** - Production Filecoin network
- **calibnet** - Public testnet (recommended for development)
- **devnet** - Lightweight local network
- **butterflynet** - Alternative testnet

Each network has its own genesis, bootstrap peers, actor bundles, and upgrade epochs.

## Common Development Workflows

### Running a Local Node

```bash
# Mainnet (requires snapshot download)
forest --encrypt-keystore false

# Calibnet (testnet, auto-download snapshot)
forest --chain calibnet --auto-download-snapshot --encrypt-keystore false

# Using Docker
docker run --init -it --rm ghcr.io/chainsafe/forest:latest --chain calibnet --auto-download-snapshot
```

### Using the CLI

```bash
# Set admin token for privileged operations
export FULLNODE_API_INFO="<token>:/ip4/127.0.0.1/tcp/2345/http"

# Or use --token flag
forest-cli --token <ADMIN_TOKEN> info show

# Common commands
forest-cli info show           # Node status
forest-cli chain head          # Current chain head
forest-cli sync status         # Sync progress
forest-cli net peers           # Connected peers
forest-cli wallet list         # List wallets
forest-cli mpool pending       # Pending messages
```

### Working with Snapshots

```bash
# Fetch snapshot
forest-tool snapshot fetch --chain calibnet

# Export snapshot
forest-tool snapshot export --output snapshot.car

# Import snapshot
forest --import-snapshot snapshot.car --encrypt-keystore false
```

### Debugging

```bash
# Enable debug logging
RUST_LOG=debug forest --chain calibnet

# Filter specific modules
RUST_LOG="debug,forest_libp2p::service=info" forest

# Use tokio-console (requires tokio-console feature)
tokio-console  # In separate terminal
forest         # With RUSTFLAGS="--cfg tokio_unstable"

# Profile with debugging symbols
FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT=1 cargo build --profile debugging
lldb target/debugging/forest
```

## Important Environment Variables

- `FOREST_KEYSTORE_PHRASE` - Passphrase for encrypted keystore (headless mode)
- `FOREST_CONFIG_PATH` - Path to config file (overrides default locations)
- `RUST_LOG` - Logging configuration (e.g., `debug`, `forest=trace`)
- `FULLNODE_API_INFO` - RPC endpoint and authentication token
- `FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT` - Disable F3 sidecar build (for debugging profile)

## Cargo Features

- **`default`** - `jemalloc`, `tokio-console`, `tracing-loki`, `tracing-chrome`
- **`test`** - Default feature set for unit tests
- **`slim`** - Minimal feature set (uses rustalloc)
- **`jemalloc`** - Use jemalloc allocator (production default)
- **`rustalloc`** - Use Rust standard allocator
- **`system-alloc`** - Use system allocator (for memory profiling)
- **`tokio-console`** - Enable tokio-console integration
- **`tracing-loki`** - Send telemetry to Loki
- **`tracing-chrome`** - Chrome tracing support
- **`no-f3-sidecar`** - Disable F3 sidecar build
- **`cargo-test`** - Group of tests that is recommended to run with `cargo test` instead of `nextest`
- **`doctest-private`** - Enable doctests for private items
- **`benchmark-private`** - Enable benchmark suite
- **`interop-tests-private`** - Enable interop tests

## Build Profiles

- **`dev`** - Fast compile, line-table-only debug info, opt-level=1 for deps
- **`quick`** - Inherits from release, opt-level=1, no LTO (good for testing)
- **`release`** - Optimized, stripped, thin LTO, panic=abort
- **`release-lto-fat`** - Maximum optimization with fat LTO
- **`debugging`** - Full debug info (requires `FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT=1`)
- **`profiling`** - For profiling tools

## Key Dependencies

- **Rust**: Version specified in `rust-toolchain.toml`
- **Go**: Version specified in `go.work` (required for F3 sidecar)
- **OS packages**: `build-essential` (Ubuntu), `clang`, `clang-devel` (Fedora)
- **mise-en-place**: Task runner and tool installer (`mise.jdx.dev`)

## Contributing Guidelines

From CONTRIBUTING.md:

- Use conventional commits (e.g., `feat: add Filecoin.RuleTheWorld RPC method`)
- Run linters before submitting: `mise lint`
- Format code: `mise fmt`
- Ensure tests pass: `mise test`
- Fill PR template exhaustively
- First-time contributors sign CLA when opening PR
- Document public functions and structs (see [Documentation practices](https://github.com/ChainSafe/forest/wiki/Documentation-practices))
