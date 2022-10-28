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
## 
FROM rust:1-buster AS build-env

# Install dependencies
RUN apt-get update && apt-get install --no-install-recommends -y build-essential clang ca-certificates curl
RUN update-ca-certificates

# Install rustup
# https://rustup.rs/
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
ENV RUSTFLAGS="-Ctarget-feature=+avx2,+fma"

WORKDIR /usr/src/forest
COPY . .

# Grab the correct toolchain
RUN rustup toolchain install nightly

ENV RUSTFLAGS="-Ctarget-feature=+avx2,+fma"

# Install Forest
RUN make install

# strip symbols to make executable smaller
RUN strip /usr/local/cargo/bin/forest /usr/local/cargo/bin/forest-cli

##
# Prod image for forest binary
##
FROM debian:10-slim

# Link package to the repository
LABEL org.opencontainers.image.source https://github.com/chainsafe/forest

# Install binary dependencies
RUN apt-get update && apt-get install --no-install-recommends -y libssl1.1 ca-certificates libcurl4
RUN update-ca-certificates

# Copy forest daemon and cli binaries from the build-env
COPY --from=build-env /usr/local/cargo/bin/forest /usr/local/bin/forest
COPY --from=build-env /usr/local/cargo/bin/forest-cli /usr/local/bin/forest-cli

ENTRYPOINT ["/usr/local/bin/forest"]
