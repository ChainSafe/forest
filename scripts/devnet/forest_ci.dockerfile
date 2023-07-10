FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install --no-install-recommends -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY forest*  /usr/local/bin/
RUN  chmod +x /usr/local/bin/forest*

# Roughly verify that the binaries work.
# This should ensure that all dynamically-linked libraries are present.
RUN forest -V && forest-cli -V
