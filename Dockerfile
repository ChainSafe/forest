# This Dockerfile is for the main forest binary
# 
# Build and run locally:
# ```
# docker build -t forest:latest -f ./Dockerfile .
# docker run --init -it forest
# ```
# 

FROM golang:1.25-bookworm AS build-env
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# install dependencies
RUN apt-get update && \
    apt-get install --no-install-recommends -y build-essential clang-20 curl git ca-certificates
RUN update-ca-certificates
ENV CC=clang-20 CXX=clang++-20

# install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /forest
COPY . .

RUN ./scripts/install_mise.sh

# Install Forest. Move it out of the cache for the prod image.
RUN --mount=type=cache,sharing=private,target=/root/.cargo/registry \
    --mount=type=cache,sharing=private,target=/root/.rustup \
    --mount=type=cache,sharing=private,target=/forest/target \
    mise trust && \
    mise run install && \
    mkdir /forest_out && \
    cp /root/.cargo/bin/forest* /forest_out

##
# Prod image for forest binary
# Use github action runner cached images to avoid being rate limited
# https://github.com/actions/runner-images/blob/main/images/linux/Ubuntu2004-Readme.md#cached-docker-images
##
# A slim image contains only forest binaries
FROM ubuntu:24.04 AS slim-image

ENV DEBIAN_FRONTEND="noninteractive"
# Install binary dependencies
RUN apt-get update && \
    apt-get install --no-install-recommends -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*
RUN update-ca-certificates

# Copy forest daemon and cli binaries from the build-env
COPY --from=build-env /forest_out/* /usr/local/bin/

# Basic verification of dynamically linked dependencies
RUN forest -V && forest-cli -V && forest-tool -V && forest-wallet -V

ENTRYPOINT ["forest"]

# A fat image contains forest binaries and fil proof parameter files under $FIL_PROOFS_PARAMETER_CACHE
FROM slim-image AS fat-image

# Move FIL_PROOFS_PARAMETER_CACHE out of forest data dir since users always need to mount the data dir
ENV FIL_PROOFS_PARAMETER_CACHE="/var/tmp/filecoin-proof-parameters"

# Populate $FIL_PROOFS_PARAMETER_CACHE
RUN forest-tool fetch-params --keys

# Cache actor bundle in the image
ENV FOREST_ACTOR_BUNDLE_PATH="/var/tmp/forest_actor_bundle.car.zst"

# Populate $FOREST_ACTOR_BUNDLE_PATH
RUN forest-tool state-migration actor-bundle $FOREST_ACTOR_BUNDLE_PATH

ENTRYPOINT ["forest"]
