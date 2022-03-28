# This Dockerfile is for the main forest binary
# Example usage:
# docker build -t forest:latest -f ./Dockerfile .
# docker run forest

FROM rust:1-buster AS build-env

WORKDIR /usr/src/forest
COPY . .

# Install dependencies
RUN apt-get update
RUN apt-get install --no-install-recommends -y build-essential clang ocl-icd-opencl-dev

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
