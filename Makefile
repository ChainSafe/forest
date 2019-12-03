lint:
	cargo clean -p node
	cargo clean -p address
	cargo clean -p crypto
	cargo clean -p blockchain
	cargo fmt
	cargo clippy -- -D warnings

build:
	cargo clean
	cargo build

release:
	cargo clean
	cargo build --release
