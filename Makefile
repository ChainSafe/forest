# Using https://github.com/tonistiigi/xx
# Use in Docker images when cross-compiling.
install-xx:
	xx-cargo install --locked --path . --force

# Redundancy tracked by #2991
install-cli:
	cargo install --locked --path . --force

install-daemon:
	cargo install --locked --path . --force

install:
	cargo install --locked --path . --force


install-quick:
	cargo install --profile quick --locked --path . --force

install-slim:
	cargo install --no-default-features --features slim --locked --path . --force

install-slim-quick:
	cargo install --profile quick --no-default-features --features slim --locked --path . --force

install-minimum:
	cargo install --no-default-features --locked --path . --force

install-minimum-quick:
	cargo install --profile quick --no-default-features --locked --path . --force

# Installs Forest binaries with default rust global allocator
install-with-rustalloc:
	cargo install --locked --path . --force --no-default-features --features rustalloc

# Installs Forest binaries with MiMalloc global allocator
install-with-mimalloc:
	cargo install --locked --path . --force --no-default-features --features mimalloc

install-lint-tools:
	cargo install --locked taplo-cli
	cargo install --locked cargo-audit
	cargo install --locked cargo-spellcheck

install-lint-tools-ci:
	wget https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz
	tar xzf cargo-binstall-x86_64-unknown-linux-musl.tgz
	cp cargo-binstall ~/.cargo/bin/cargo-binstall

	cargo binstall --no-confirm taplo-cli cargo-spellcheck cargo-audit

install-doc-tools:
	cargo install --locked mdbook
	cargo install --locked mdbook-linkcheck

clean-all:
	cargo clean

clean:
	cargo clean

# Lints with everything we have in our CI arsenal
lint-all: lint audit spellcheck

audit:
	cargo audit || (echo "See .config/audit.toml"; false)

spellcheck:
	cargo spellcheck --code 1 || (echo "See .config/spellcheck.md for tips"; false)

lint: license clean lint-clippy
	cargo fmt --all --check
	taplo fmt --check
	taplo lint

# Don't bother linting different allocators
# Don't lint all permutations, just different versions of database, cns
# This should be simplified in #2984
# --quiet: don't show build logs
lint-clippy:
	cargo clippy --all-targets --quiet --no-deps -- --deny=warnings
	cargo clippy --all-targets --no-default-features --features slim --quiet --no-deps -- --deny=warnings
	cargo clippy --all-targets --no-default-features --quiet --no-deps -- --deny=warnings
	cargo clippy --benches --features benchmark-private --quiet --no-deps -- --deny=warnings

DOCKERFILES=$(wildcard Dockerfile*)
lint-docker: $(DOCKERFILES)
	docker run --rm -i hadolint/hadolint < $<

# Formats Rust, TOML and Markdown files.
fmt:
	cargo fmt --all
	taplo fmt
	yarn md-fmt

build:
	cargo build

release:
	cargo build --release

docker-run:
	docker build -t forest:latest -f ./Dockerfile . && docker run forest

test: go-mod
	cargo nextest run

	# nextest doesn't run doctests https://github.com/nextest-rs/nextest/issues/16
	# see also lib.rs::doctest_private
	cargo test --doc --features doctest-private

test-release: go-mod
	cargo nextest run --release

test-all: test test-release

go-mod:
	(cd $(PWD)/src/libp2p_bitswap/tests/go-app && go mod vendor && go build -o /tmp/forest-go-bitswap-compat-test) && \
	(cd $(PWD)/src/libp2p/tests/go-kad && go mod vendor && go build -o /tmp/forest-go-kad-compat-test) || \
	(echo "Some tests require Go 1.21.x to be installed, follow instructions at https://go.dev/dl/" && exit 1)

# Checks if all headers are present and adds if not
license:
	./scripts/add_license.sh

docs:
	cargo doc --no-deps

mdbook:
	mdbook serve documentation

mdbook-build:
	mdbook build ./documentation

# These are the docs that are hosted at https://chainsafe.github.io/forest/rustdoc/.
# The root index.html simply redirects to the main `forest-filecoin` library documentation.
vendored-docs:
	cargo doc --document-private-items
	cp ./build/vendored-docs-redirect.index.html target/doc/index.html

.PHONY: $(MAKECMDGOALS)
