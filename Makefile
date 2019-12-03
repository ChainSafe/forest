lint:
	cargo clean --manifest-path ./Cargo.toml
	cargo fmt
	cargo clippy -- -D warnings
