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
FROM ubuntu:20.04 AS build-env

# Install dependencies
RUN apt-get update && apt-get install --no-install-recommends -y build-essential clang ocl-icd-opencl-dev cmake

WORKDIR /usr/src/forest
COPY . .

# Install rustup
# https://rustup.rs/
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Forest
RUN make install

# strip symbols to make executable smaller
RUN strip /usr/local/cargo/bin/forest /usr/local/cargo/bin/forest-cli

##
# Prod image for forest binary
# Use github action runner cached images to avoid being rate limited
# https://github.com/actions/runner-images/blob/main/images/linux/Ubuntu2004-Readme.md#cached-docker-images
##
FROM ubuntu:20.04

# Link package to the repository
LABEL org.opencontainers.image.source https://github.com/chainsafe/forest

# Install binary dependencies
RUN apt-get update && apt-get install --no-install-recommends -y ocl-icd-opencl-dev libssl1.1 ca-certificates libcurl4
RUN update-ca-certificates

# Copy forest daemon and cli binaries from the build-env
COPY --from=build-env /usr/local/cargo/bin/forest /usr/local/bin/forest
COPY --from=build-env /usr/local/cargo/bin/forest-cli /usr/local/bin/forest-cli

ENTRYPOINT ["/usr/local/bin/forest"]
