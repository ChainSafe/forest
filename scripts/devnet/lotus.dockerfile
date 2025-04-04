# Lotus binaries image, to be used in the local devnet with Forest.
FROM golang:1.23-bookworm AS lotus-builder
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update && \
    apt-get install --no-install-recommends -y curl ca-certificates build-essential clang ocl-icd-opencl-dev ocl-icd-libopencl1 jq libhwloc-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /lotus

# Install rust toolchain for rebuilding `filecoin-ffi`
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal

ENV PATH="/root/.cargo/bin:${PATH}"

# TODO - we'll need to update and push to GHCR once its there
RUN git clone --depth 1 https://github.com/filecoin-project/lotus.git . && git reset --hard 3d0018c

# https://github.com/Filecoin-project/filecoin-ffi?tab=readme-ov-file#building-from-source
RUN CGO_CFLAGS_ALLOW="-D__BLST_PORTABLE__" \
    CGO_CFLAGS="-D__BLST_PORTABLE__" \
    FFI_USE_BLST_PORTABLE="1" \
    FFI_USE_GPU="0" \
    make 2k && strip lotus*

FROM ubuntu:24.04

# Needed for the healthcheck
RUN apt-get update && \
    apt-get install --no-install-recommends -y curl && \
    rm -rf /var/lib/apt/lists/*

# Need to copy the relevant shared libraries from the builder image.
# See https://github.com/filecoin-project/lotus/blob/master/Dockerfile
COPY --from=lotus-builder /etc/ssl/certs            /etc/ssl/certs
COPY --from=lotus-builder /lib/*/libdl.so.2         /lib/
COPY --from=lotus-builder /lib/*/librt.so.1         /lib/
COPY --from=lotus-builder /lib/*/libgcc_s.so.1      /lib/
COPY --from=lotus-builder /lib/*/libutil.so.1       /lib/
COPY --from=lotus-builder /usr/lib/*/libltdl.so.7   /lib/
COPY --from=lotus-builder /usr/lib/*/libnuma.so.1   /lib/
COPY --from=lotus-builder /usr/lib/*/libhwloc.so.*  /lib/
COPY --from=lotus-builder /usr/lib/*/libOpenCL.so.1 /lib/

# Copy only the binaries relevant for the devnet
COPY --from=lotus-builder /lotus/lotus /lotus/lotus-miner /lotus/lotus-seed /lotus/lotus-shed /usr/local/bin/

WORKDIR /lotus

# Basic verification of dynamically linked dependencies
RUN lotus -v

CMD ["/bin/bash"]
