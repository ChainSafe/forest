clean-all:
	cargo clean

clean:
	@echo "Cleaning local packages..."
	@cargo clean -p node
	@cargo clean -p ferret-libp2p
	@cargo clean -p network
	@cargo clean -p blockchain
	@cargo clean -p vm
	@cargo clean -p address
	@cargo clean -p crypto
	@cargo clean -p encoding
	@echo "Done cleaning."

lint: clean
	cargo fmt
	cargo clippy -- -D warnings

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

.PHONY: clean clean-all lint build release test