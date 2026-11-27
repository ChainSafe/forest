# Curio 2k image for the Forest-produced Curio devnet. Tags and GOAMD64 must be
# set explicitly: Curio's Dockerfile clears CURIO_TAGS, and its Makefile hardcodes
# GOAMD64=v3 which SIGILLs under QEMU.
ARG CURIO_COMMIT
FROM rust:1.86.0-slim-bookworm AS rust-toolchain

FROM golang:1.26.10-trixie AS curio-builder
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
ARG CURIO_COMMIT
RUN test -n "$CURIO_COMMIT"

RUN apt-get update && \
    apt-get install --no-install-recommends -y ca-certificates build-essential clang \
      ocl-icd-opencl-dev ocl-icd-libopencl1 jq libhwloc-dev git && \
    rm -rf /var/lib/apt/lists/*

COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

WORKDIR /opt/curio
RUN git clone https://github.com/filecoin-project/curio.git . && \
    git checkout "$CURIO_COMMIT" && \
    git submodule update --init

# Portable BLST so the binary runs under QEMU; rewrite GOAMD64=v3 -> v1.
RUN sed -i 's/GOAMD64=v3/GOAMD64=v1/g' scripts/makefiles/30-build.mk

ENV FFI_USE_OPENCL=1 \
    CURIO_TAGS="cunative nosupraseal 2k" \
    CGO_CFLAGS_ALLOW="-D__BLST_PORTABLE__" \
    CGO_CFLAGS="-D__BLST_PORTABLE__" \
    FFI_USE_BLST_PORTABLE=1 \
    FFI_USE_GPU=0

RUN make clean deps && make build && strip curio sptool

FROM ubuntu:24.04
RUN apt-get update && \
    apt-get install --no-install-recommends -y ca-certificates curl jq dnsutils && \
    rm -rf /var/lib/apt/lists/*

COPY --from=curio-builder /etc/ssl/certs /etc/ssl/certs
COPY --from=curio-builder /lib/*/libdl.so.2 /lib/
COPY --from=curio-builder /lib/*/librt.so.1 /lib/
COPY --from=curio-builder /lib/*/libgcc_s.so.1 /lib/
COPY --from=curio-builder /lib/*/libutil.so.1 /lib/
COPY --from=curio-builder /usr/lib/*/libltdl.so.7 /lib/
COPY --from=curio-builder /usr/lib/*/libnuma.so.1 /lib/
COPY --from=curio-builder /usr/lib/*/libhwloc.so.* /lib/
COPY --from=curio-builder /usr/lib/*/libOpenCL.so.1 /lib/
COPY --from=curio-builder /opt/curio/curio /opt/curio/sptool /usr/local/bin/

RUN curio --version && ! command -v lotus-miner

CMD ["/bin/bash"]
