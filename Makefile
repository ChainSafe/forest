clean-all:
	cargo clean

clean:
	@echo "Cleaning local packages..."
	@cargo clean -p node
	@cargo clean -p clock
	@cargo clean -p ferret-libp2p
	@cargo clean -p network
	@cargo clean -p blockchain
	@cargo clean -p vm
	@cargo clean -p address
	@cargo clean -p actor
	@cargo clean -p message
	@cargo clean -p runtime
	@cargo clean -p state_tree
	@cargo clean -p interpreter
	@cargo clean -p crypto
	@cargo clean -p encoding
	@echo "Done cleaning."

lint: clean
	cargo fmt
	cargo clippy -- -D warnings
	./scripts/add_license.sh

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

license:
	./scripts/add_license.sh

.PHONY: clean clean-all lint build release test license
