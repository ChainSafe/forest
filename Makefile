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
	@cargo clean -p ipld_amt
	@cargo clean -p forest_bigint
	@echo "Done cleaning."

lint: clean license
	cargo fmt
	cargo clippy -- -D warnings

install:
	cargo install --path forest

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

license:
	./scripts/add_license.sh

docs:
	cargo doc --no-deps --all-features

.PHONY: clean clean-all lint build release test license
