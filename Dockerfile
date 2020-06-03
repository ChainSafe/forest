# This Dockerfile is for the main forest binary
# Example usage:
# docker build -t forest:latest -f ./Dockerfile .
# docker run forest

FROM rust:1.42-stretch AS build-env

WORKDIR /usr/src/forest
COPY . .

# Install protoc
ENV PROTOC_ZIP protoc-3.7.1-linux-x86_64.zip
RUN curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v3.7.1/$PROTOC_ZIP
RUN unzip -o $PROTOC_ZIP -d /usr/local bin/protoc
RUN unzip -o $PROTOC_ZIP -d /usr/local 'include/*'
RUN rm -f $PROTOC_ZIP

# Extra dependencies needed for rust-fil-proofs
RUN apt-get update && \
    apt-get install -y curl file gcc g++ git make openssh-client \
    autoconf automake cmake libtool libcurl4-openssl-dev libssl-dev \
    libelf-dev libdw-dev binutils-dev zlib1g-dev libiberty-dev wget \
    xz-utils pkg-config python clang ocl-icd-opencl-dev

RUN cargo install --path forest

# Prod image for forest binary
FROM debian:buster-slim

# Copy over binaries from the build-env
COPY --from=build-env /usr/local/cargo/bin/forest /usr/local/bin/forest

CMD ["forest"]
