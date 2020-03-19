SER_TESTS = "tests/serialization_tests"

clean-all:
	cargo clean

clean:
	@echo "Cleaning local packages..."
	@cargo clean -p forest
	@cargo clean -p node
	@cargo clean -p clock
	@cargo clean -p forest_libp2p
	@cargo clean -p blockchain
	@cargo clean -p forest_blocks
	@cargo clean -p chain_sync
	@cargo clean -p vm
	@cargo clean -p forest_address
	@cargo clean -p actor
	@cargo clean -p forest_message
	@cargo clean -p runtime
	@cargo clean -p state_tree
	@cargo clean -p state_manager
	@cargo clean -p interpreter
	@cargo clean -p crypto
	@cargo clean -p forest_encoding
	@cargo clean -p forest_cid
	@cargo clean -p forest_ipld
	@cargo clean -p ipld_hamt
	@cargo clean -p ipld_amt
	@cargo clean -p forest_bigint
	@echo "Done cleaning."

lint: license clean
	cargo fmt --all
	cargo clippy -- -D warnings

install:
	cargo install --path forest --force

build:
	cargo build

release:
	cargo build --release

# Git submodule test vectors
pull-serialization-tests:
	git submodule update --init

run-vectors:
	cargo test --release --manifest-path=$(SER_TESTS)/Cargo.toml --features "serde_tests"

test-vectors: pull-serialization-tests run-vectors

# Test all without the submodule test vectors with release configuration
test:
	cargo test --all --release --exclude serialization_tests

# This will run all tests will all features enabled, which will exclude some tests with
# specific features disabled
test-all: pull-serialization-tests
	cargo test --all-features

# Checks if all headers are present and adds if not
license:
	./scripts/add_license.sh

docs:
	cargo doc --no-deps --all-features

.PHONY: clean clean-all lint build release test license test-all test-vectors run-vectors pull-serialization-tests
