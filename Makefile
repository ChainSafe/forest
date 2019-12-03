lint:
	cargo clean -p vm
	cargo clean -p address
	cargo clean -p node
	cargo clean -p crypto
	cargo clean -p blockchain
	cargo fmt
	cargo clippy -- -D warnings
