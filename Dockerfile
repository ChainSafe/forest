# This Dockerfile is for the main forest binary
# 
# Build and run locally:
# ```
# docker build -t forest:latest -f ./Dockerfile .
# docker run --init -it forest
# ```
# 
# Build and manually push to Github Container Registry (see https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)
# ```
# docker build -t ghcr.io/chainsafe/forest:latest .
# docker push ghcr.io/chainsafe/forest:latest
# ```

##
# Build stage
# Use github action runner cached images to avoid being rate limited
# https://github.com/actions/runner-images/blob/main/images/linux/Ubuntu2004-Readme.md#cached-docker-images
## 

# Cross-compilation helpers
# https://github.com/tonistiigi/xx
FROM --platform=$BUILDPLATFORM ghcr.io/lesnyrumcajs/xx:1.2.1 AS xx

FROM --platform=$BUILDPLATFORM ubuntu:22.04 AS build-env
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# install dependencies
RUN apt-get update && \
    apt-get install --no-install-recommends -y build-essential clang cmake curl ca-certificates

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

# Copy the cross-compilation scripts 
COPY --from=xx / /

# export TARGETPLATFORM
ARG TARGETPLATFORM

# Install those packages for the target architecture
RUN xx-apt-get update && \
    xx-apt-get install -y libc6-dev g++

WORKDIR /forest
COPY . .

# Install Forest. Move it out of the cache for the prod image.
RUN --mount=type=cache,sharing=private,target=/root/.cargo/registry \
    --mount=type=cache,sharing=private,target=/root/.rustup \
    --mount=type=cache,sharing=private,target=/forest/target \
    make install-xx && \
    mkdir /forest_out && \
    cp /root/.cargo/bin/forest* /forest_out

##
# Prod image for forest binary
# Use github action runner cached images to avoid being rate limited
# https://github.com/actions/runner-images/blob/main/images/linux/Ubuntu2004-Readme.md#cached-docker-images
##
FROM ubuntu:22.04

ARG SERVICE_USER=forest
ARG SERVICE_GROUP=forest
ARG DATA_DIR=/home/forest/.local/share/forest

ENV DEBIAN_FRONTEND="noninteractive"
# Install binary dependencies
RUN apt-get update && \
    apt-get install --no-install-recommends -y aria2 ca-certificates && \
    rm -rf /var/lib/apt/lists/*
RUN update-ca-certificates

# Create user and group and assign appropriate rights to the forest binaries
RUN addgroup --gid 1000 ${SERVICE_GROUP} && \
    adduser --uid 1000 --ingroup ${SERVICE_GROUP} --disabled-password --gecos "" ${SERVICE_USER}

# Copy forest daemon and cli binaries from the build-env
COPY --from=build-env --chown=${SERVICE_USER}:${SERVICE_GROUP} /forest_out/* /usr/local/bin/

# Initialize data directory with proper permissions
RUN mkdir -p ${DATA_DIR} && \
    chown -R ${SERVICE_USER}:${SERVICE_GROUP} ${DATA_DIR}

USER ${SERVICE_USER}
WORKDIR /home/${SERVICE_USER}

# Basic verification of dynamically linked dependencies
RUN forest -V && forest-cli -V

ENTRYPOINT ["forest"]
