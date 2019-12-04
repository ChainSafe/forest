clean-all:
	cargo clean

clean:
	cargo clean -p node
	cargo clean -p address
	cargo clean -p crypto
	cargo clean -p blockchain
	cargo clean -p encoding

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