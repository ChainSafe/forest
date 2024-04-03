# Lotus binaries image, to be used in the local devnet with Forest.
FROM golang:1.21.7-bullseye AS lotus-builder

RUN apt-get update && apt-get install -y ca-certificates build-essential clang ocl-icd-opencl-dev ocl-icd-libopencl1 jq libhwloc-dev 

WORKDIR /lotus

RUN git clone --depth 1 --branch v1.26.0 https://github.com/filecoin-project/lotus.git .

RUN CGO_CFLAGS_ALLOW="-D__BLST_PORTABLE__" \
    CGO_CFLAGS="-D__BLST_PORTABLE__" \
    make 2k && strip lotus*

FROM ubuntu:22.04

# Needed for the healthcheck
RUN apt-get update && apt-get install -y curl

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
COPY --from=lotus-builder /lotus/lotus /lotus/lotus-miner /lotus/lotus-seed /usr/local/bin/

WORKDIR /lotus

# Basic verification of dynamically linked dependencies
RUN lotus -v

CMD ["/bin/bash"]
