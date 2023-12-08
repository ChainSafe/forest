FROM ubuntu:22.04
FROM rust:1.74

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get -y update && apt-get -y install git
RUN cargo install rust-script
RUN apt -y install build-essential clang time
RUN git clone https://github.com/ChainSafe/forest.git
# TODO: remove this step before merging; for testing purposes only and the script willl handle this
#RUN cd forest && make install
# TODO: remove this step before merging; file will be located in repo
COPY benchmarking.rs /forest/scripts/benchmarking/
RUN  chmod +x /forest/scripts/benchmarking/benchmarking.rs

#CMD ["echo", "Hello World"]