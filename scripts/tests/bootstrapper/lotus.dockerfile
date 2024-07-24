# Lotus binaries image, to be used in the local devnet with Forest.
FROM golang:1.21-bullseye AS lotus-builder

RUN apt-get update && apt-get install -y curl ca-certificates build-essential clang ocl-icd-opencl-dev ocl-icd-libopencl1 jq libhwloc-dev 

WORKDIR /lotus

# Install rust toolchain for rebuilding `filecoin-ffi`
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal

ENV PATH="/root/.cargo/bin:${PATH}"

ARG LOTUS_VERSION
RUN git clone --depth 1 --branch ${LOTUS_VERSION} https://github.com/filecoin-project/lotus.git .

# Replace the default bootstrap peers with a pre-defined one. This is needed on this level because the bootstrap peers are compiled into the binary and not configurable at runtime.
ARG BOOTSTRAPPER
RUN echo ${BOOTSTRAPPER} > build/bootstrap/calibnet.pi

# Apply a patch to Lotus so that local IPs are not discarded
COPY patch-lotus.diff .
RUN git apply patch-lotus.diff

# https://github.com/Filecoin-project/filecoin-ffi?tab=readme-ov-file#building-from-source
RUN CGO_CFLAGS_ALLOW="-D__BLST_PORTABLE__" \
    CGO_CFLAGS="-D__BLST_PORTABLE__" \
    FFI_USE_BLST_PORTABLE="1" \
    FFI_USE_GPU="0" \
    make calibnet && strip lotus*

FROM ubuntu:22.04

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

# Copy only the binaries relevant for the bootstrap test
COPY --from=lotus-builder /lotus/lotus /usr/local/bin/

WORKDIR /lotus

ENV LOTUS_SYNC_BOOTSTRAP_PEERS=1

# Basic verification of dynamically linked dependencies
RUN lotus -v

CMD ["/bin/bash"]
