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

lint-go:
	go run github.com/golangci/golangci-lint/v2/cmd/golangci-lint@v2.3.1 run ./f3-sidecar ./interop-tests/src/tests/go_app

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
	cargo nextest run --workspace --no-fail-fast -- --skip rpc_snapshot_test_
	cargo test --package forest-filecoin --lib rpc_snapshot_test_
	# nextest doesn't run doctests https://github.com/nextest-rs/nextest/issues/16
	# see also lib.rs::doctest_private
	cargo test --doc --features doctest-private

test-release:
	cargo nextest run --cargo-profile quick --workspace --no-fail-fast -- --skip rpc_snapshot_test_
	cargo test --package forest-filecoin --profile quick --lib rpc_snapshot_test_

test-all: test test-release

# Checks if all headers are present and adds if not
license:
	./scripts/add_license.sh

docs:
	cargo doc --no-deps

##
## Memory Profiling
##

# Read up on memory profiling in Forest: https://rumcajs.dev/posts/memory-analysis-in-rust/

# Memory profiling is done with the `profiling` profile. There's no silver bullet for memory profiling, so we provide a few options here.

### Gperftools
# https://github.com/gperftools/gperftools

# Profile with gperftools (Memory/Heap profiler)
# There is a workaround there, as outlined in https://github.com/gperftools/gperftools/issues/1603
gperfheapprofile = FOREST_PROFILING_GPERFTOOLS_BUILD=1 cargo build --no-default-features --features system-alloc --profile=profiling --bin $(1); \
	ulimit -n 8192; \
	HEAPPROFILE_USE_PID=t HEAPPROFILE=/tmp/gperfheap.$(1).prof target/profiling/$(1) $(2)

gperfheapprofile.forest:
	$(call gperfheapprofile,forest, --chain calibnet --encrypt-keystore=false)

# To visualize the heap profile, run:
# pprof -http=localhost:8080 <path/to/profiled/binary> <path/to/gperfheap.forest.prof
# Don't use the default `pprof` package; use the one from google instead: https://github.com/google/pprof

#### Heaptrack
# https://github.com/KDE/heaptrack

memprofile-heaptrack = cargo build --no-default-features --features system-alloc --profile=profiling --bin $(1); \
						 ulimit -n 8192; \
             heaptrack -o /tmp/heaptrack.$(1).%p.zst target/profiling/$(1) $(2)

memprofile-heaptrack.forest:
	$(call memprofile-heaptrack,forest, --chain calibnet --encrypt-keystore=false)

.PHONY: $(MAKECMDGOALS)
