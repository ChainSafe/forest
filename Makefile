SER_TESTS = "tests/serialization_tests"
CONF_TESTS = "tests/conformance_tests"

install:
	cargo install --locked --path forest --force

clean-all:
	cargo clean

clean:
	@echo "Cleaning local packages..."
	@cargo clean -p forest
	@cargo clean -p fil_clock
	@cargo clean -p forest_libp2p
	@cargo clean -p forest_blocks
	@cargo clean -p chain_sync
	@cargo clean -p forest_vm
	@cargo clean -p forest_address
	@cargo clean -p actor
	@cargo clean -p forest_message
	@cargo clean -p runtime
	@cargo clean -p state_tree
	@cargo clean -p state_manager
	@cargo clean -p interpreter
	@cargo clean -p forest_crypto
	@cargo clean -p forest_encoding
	@cargo clean -p forest_cid
	@cargo clean -p forest_ipld
	@cargo clean -p ipld_hamt
	@cargo clean -p ipld_amt
	@cargo clean -p forest_bigint
	@cargo clean -p bitfield
	@cargo clean -p commcid
	@cargo clean -p fil_types
	@cargo clean -p ipld_blockstore
	@cargo clean -p rpc
	@cargo clean -p key_management
	@cargo clean -p forest_json_utils
	@cargo clean -p test_utils
	@cargo clean -p message_pool
	@cargo clean -p genesis
	@echo "Done cleaning."

lint: license clean
	cargo fmt --all
	cargo clippy --all-features -- -D warnings

build:
	cargo build --bin forest

release:
	cargo build --release --bin forest

docker-run:
	docker build -t forest:latest -f ./Dockerfile . && docker run forest

# Git submodule test vectors
pull-serialization-tests:
	git submodule update --init

run-serialization-vectors:
	cargo test --release --manifest-path=$(SER_TESTS)/Cargo.toml --features "submodule_tests"

run-conformance-vectors:
	cargo test --release --manifest-path=$(CONF_TESTS)/Cargo.toml --features "submodule_tests" -- --nocapture

run-vectors: run-serialization-vectors run-conformance-vectors

test-vectors: pull-serialization-tests run-vectors

# Test all without the submodule test vectors with release configuration
test:
	cargo test --all --all-features --exclude serialization_tests --exclude conformance_tests

# This will run all tests will all features enabled, which will exclude some tests with
# specific features disabled
test-all: pull-serialization-tests
	cargo test --all-features

# This will run all tests will all features enabled, which will exclude some tests with
# specific features disabled with verbose compiler output
test-all-verbose: pull-serialization-tests
	cargo test --verbose --all-features

test-all-no-run: pull-serialization-tests
	cargo test --all-features --no-run

# Checks if all headers are present and adds if not
license:
	./scripts/add_license.sh

docs:
	cargo doc --no-deps --all-features

.PHONY: clean clean-all lint build release test license test-all test-vectors run-vectors pull-serialization-tests install docs
