# This Dockerfile is for the main forest binary
# Example usage:
# docker build -t forest:latest -f ./Dockerfile .
# docker run forest

FROM rust:1.59.0-slim-buster AS build-env

ENV RUST_MIN_VERSION=1.59.0

# Install dependencies
RUN apt-get update
RUN apt-get install -y apt-utils \
  && apt-get install --no-install-recommends -y build-essential pkg-config m4 clang libssl-dev ocl-icd-opencl-dev

WORKDIR /usr/src/forest
COPY . .

# Check Toolchain
RUN echo "* Operating System:" && cat /etc/os-release\
  && echo "* Rust Compiler Version:" && rustc --version\
  && echo "* RustUp Version:" && rustup --version\
  && echo "* RustUp Show:" && rustup show\
  && echo "* RustUp Toolchains Installed:" && rustup toolchain list \
  && echo "* RustUp Components Installed:" && rustup component list --installed

# Install required Toolchain
RUN echo "* Requested Rust Compiler Version:" $RUST_MIN_VERSION \
  && echo "* Unset Rust Compiler Overrides ..." \
  && rustup override unset \
  && echo "* Override Default Rust Compiler Version ..." \
  && rustup override set $RUST_MIN_VERSION \
  && echo "* Install Rust Compiler requested Version ..." \
  && rustup install $RUST_MIN_VERSION \
  && rustup target add wasm32-unknown-unknown \
  && echo "* Rust Compiler Version:" && rustc --version \
  && echo "* RustUp Show:" && rustup show

# Install Forest
RUN cargo install --path forest

# Prod image for forest binary
FROM debian:buster-slim

# Install binary dependencies
RUN apt-get update
RUN apt-get install --no-install-recommends -y build-essential clang ocl-icd-opencl-dev

# Copy over binaries from the build-env
COPY --from=build-env /usr/local/cargo/bin/forest /usr/local/bin/forest

CMD ["forest"]
