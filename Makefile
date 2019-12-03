clean-all:
	cargo clean

clean:
	cargo clean -p node
	cargo clean -p address
	cargo clean -p crypto
	cargo clean -p blockchain

lint: clean
	cargo fmt
	cargo clippy -- -D warnings

build:
	cargo build

release:
	cargo build --release
