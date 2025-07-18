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

install-lto-fat:
	cargo install --locked --force --profile release-lto-fat --path .

install-minimum-quick:
	cargo install --profile quick --no-default-features --locked --path . --force

# Installs Forest binaries with default rust global allocator
install-with-rustalloc:
	cargo install --locked --path . --force --no-default-features --features rustalloc

install-lint-tools:
	cargo install --locked taplo-cli
	cargo install --locked cargo-deny
	cargo install --locked cargo-spellcheck

# Denotes the architecture of the machine. This is required for direct binary downloads.
# Note that some repositories might use different names for the same architecture.
CPU_ARCH := $(shell \
  ARCH=$$(uname -m); \
  if [ "$$ARCH" = "arm64" ]; then \
    ARCH="aarch64"; \
  fi; \
  echo "$$ARCH" \
)

install-cargo-binstall:
	wget https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-$(CPU_ARCH)-unknown-linux-musl.tgz
	tar xzf cargo-binstall-$(CPU_ARCH)-unknown-linux-musl.tgz
	cp cargo-binstall ~/.cargo/bin/cargo-binstall

install-lint-tools-ci: install-cargo-binstall
	cargo binstall --no-confirm taplo-cli cargo-spellcheck cargo-deny

clean:
	cargo clean

# Lints with everything we have in our CI arsenal
lint-all: lint deny spellcheck

deny:
	cargo deny check bans licenses sources || (echo "See deny.toml"; false)

spellcheck:
	cargo spellcheck --code 1 || (echo "See .config/spellcheck.md for tips"; false)

lint: license clean lint-clippy
	cargo fmt --all --check
	taplo fmt --check
	taplo lint

# Don't bother linting different allocators
# --quiet: don't show build logs
lint-clippy:
	cargo clippy --all-targets --quiet --no-deps -- --deny=warnings
	cargo clippy --all-targets --no-default-features --features slim --quiet --no-deps -- --deny=warnings
	cargo clippy --all-targets --no-default-features --quiet --no-deps -- --deny=warnings
	cargo clippy --benches --features benchmark-private --quiet --no-deps -- --deny=warnings
	# check docs.rs build
	DOCS_RS=1 cargo clippy --all-targets --quiet --no-deps -- --deny=warnings

DOCKERFILES=$(wildcard Dockerfile*)
lint-docker: $(DOCKERFILES)
	docker run --rm -i hadolint/hadolint < $<

# Formats Rust, TOML and Markdown files.
fmt:
	cargo fmt --all
	taplo fmt
	corepack enable && yarn && yarn md-fmt

md-check:
	corepack enable && yarn && yarn md-check

build:
	cargo build

release:
	cargo build --release

docker-run:
	docker build -t forest:latest -f ./Dockerfile . && docker run forest

test:
	cargo nextest run --workspace --no-fail-fast

	# nextest doesn't run doctests https://github.com/nextest-rs/nextest/issues/16
	# see also lib.rs::doctest_private
	cargo test --doc --features doctest-private

test-release:
	cargo nextest run --cargo-profile quick --workspace --no-fail-fast

test-all: test test-release

# Checks if all headers are present and adds if not
license:
	./scripts/add_license.sh

docs:
	cargo doc --no-deps

.PHONY: $(MAKECMDGOALS)
