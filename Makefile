SER_TESTS = "tests/serialization_tests"

# Using https://github.com/tonistiigi/xx
# Use in Docker images when cross-compiling.
install-xx:
	xx-cargo install --locked --path forest/cli --force
	xx-cargo install --locked --path forest/daemon --force

install-cli:
	cargo install --locked --path forest/cli --force

install-daemon:
	cargo install --locked --path forest/daemon --force

install: install-cli install-daemon

# Installs Forest binaries with RocksDb backend
install-with-rocksdb:
	cargo install --locked --path forest/daemon --force --no-default-features --features forest_fil_cns,rocksdb
	cargo install --locked --path forest/cli --force --no-default-features --features rocksdb

# Installs Forest binaries with Jemalloc global allocator
install-with-jemalloc:
	cargo install --locked --path forest/daemon --force --features jemalloc
	cargo install --locked --path forest/cli --force --features jemalloc

# Installs Forest binaries with MiMalloc global allocator
install-with-mimalloc:
	cargo install --locked --path forest/daemon --force --features mimalloc
	cargo install --locked --path forest/cli --force --features mimalloc

install-deps:
	apt-get update -y
	apt-get install --no-install-recommends -y build-essential clang protobuf-compiler ocl-icd-opencl-dev aria2 cmake

install-lint-tools:
	cargo install --locked taplo-cli
	cargo install --locked cargo-audit
	cargo install --locked cargo-spellcheck
	cargo install --locked cargo-udeps

install-doc-tools:
	cargo install --locked mdbook
	cargo install --locked mdbook-linkcheck

clean-all:
	cargo clean

clean:
	@echo "Cleaning local packages..."
	@cargo clean -p forest-cli
	@cargo clean -p forest-daemon
	@cargo clean -p forest_cli_shared
	@cargo clean -p forest_libp2p
	@cargo clean -p forest_blocks
	@cargo clean -p forest_chain_sync
	@cargo clean -p forest_message
	@cargo clean -p forest_state_manager
	@cargo clean -p forest_interpreter
	@cargo clean -p forest_crypto
	@cargo clean -p forest_encoding
	@cargo clean -p forest_ipld
	@cargo clean -p forest_json
	@cargo clean -p forest_fil_types
	@cargo clean -p forest_rpc
	@cargo clean -p forest_key_management
	@cargo clean -p forest_utils
	@cargo clean -p forest_test_utils
	@cargo clean -p forest_message_pool
	@cargo clean -p forest_genesis
	@cargo clean -p forest_networks
	@echo "Done cleaning."

# Lints with everything we have in our CI arsenal
lint-all: lint audit spellcheck udeps

audit:
	cargo audit --ignore RUSTSEC-2020-0071

udeps:
	cargo udeps --all-targets --features submodule_tests,instrumented_kernel

spellcheck:
	cargo spellcheck --code 1

lint: license clean lint-clippy
	cargo fmt --all --check
	taplo fmt --check
	taplo lint
	
lint-clippy:
	cargo clippy --features mimalloc
	cargo clippy --features jemalloc
	cargo clippy -p forest_libp2p_bitswap --all-targets -- -D warnings -W clippy::unused_async -W clippy::redundant_else
	cargo clippy -p forest_libp2p_bitswap --all-targets --features tokio -- -D warnings -W clippy::unused_async -W clippy::redundant_else
	cargo clippy --features slow_tests,submodule_tests --all-targets -- -D warnings -W clippy::unused_async -W clippy::redundant_else
	cargo clippy --all-targets --no-default-features --features forest_deleg_cns,rocksdb,instrumented_kernel -- -D warnings -W clippy::unused_async -W clippy::redundant_else

# Formats Rust and TOML files
fmt:
	cargo fmt --all
	taplo fmt

build:
	cargo build --bin forest --bin forest-cli

release:
	cargo build --release --bin forest --bin forest-cli

docker-run:
	docker build -t forest:latest -f ./Dockerfile . && docker run forest

# Git submodule test vectors
pull-serialization-tests:
	git submodule update --init

run-serialization-vectors:
	cargo nextest run --manifest-path=$(SER_TESTS)/Cargo.toml --features submodule_tests

run-vectors: run-serialization-vectors

test-vectors: pull-serialization-tests run-vectors

# Test all without the submodule test vectors with release configuration
test:
	cargo nextest run --all --exclude serialization_tests --exclude forest_message --exclude forest_crypto
	cargo nextest run -p forest_crypto --features blst --no-default-features
	cargo nextest run -p forest_message --features blst --no-default-features
	cargo nextest run -p forest_db --no-default-features --features paritydb
	cargo nextest run -p forest_db --no-default-features --features rocksdb
	cargo nextest run -p forest_libp2p_bitswap --all-features
	cargo check --tests --features slow_tests

test-slow:
	cargo nextest run -p forest_message_pool --features slow_tests
	cargo nextest run -p forest-cli --features slow_tests
	cargo nextest run -p forest-daemon --features slow_tests

test-release:
	cargo nextest run --release --all --exclude serialization_tests --exclude forest_message --exclude forest_crypto
	cargo nextest run --release -p forest_crypto --features blst --no-default-features
	cargo nextest run --release -p forest_message --features blst --no-default-features
	cargo nextest run --release -p forest_db --no-default-features --features paritydb
	cargo nextest run --release -p forest_db --no-default-features --features rocksdb
	cargo check --tests --features slow_tests

test-slow-release:
	cargo nextest run --release -p forest_message_pool --features slow_tests
	cargo nextest run --release -p forest-cli --features slow_tests
	cargo nextest run --release -p forest-daemon --features slow_tests

smoke-test:
	./scripts/smoke_test.sh

test-all: test test-vectors test-slow

test-all-release: test-release test-vectors test-slow-release

# Checks if all headers are present and adds if not
license:
	./scripts/add_license.sh

docs:
	cargo doc --no-deps

mdbook:
	mdbook serve documentation

mdbook-build:
	mdbook build ./documentation

rustdoc:
	cargo doc --workspace --no-deps

.PHONY: clean clean-all lint lint-clippy build release test test-all test-all-release test-release license test-vectors run-vectors pull-serialization-tests install-cli install-daemon install install-deps install-lint-tools docs run-serialization-vectors rustdoc
