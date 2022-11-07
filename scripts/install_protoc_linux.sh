#! /usr/bin/sh

PROTOC_VERSION=$(curl -s "https://api.github.com/repos/protocolbuffers/protobuf/releases/latest" | grep -Po '"tag_name": "v\K[0-9.]+')
echo "${PROTOC_VERSION}"
curl -Lo /tmp/protoc.zip "https://github.com/protocolbuffers/protobuf/releases/latest/download/protoc-${PROTOC_VERSION}-linux-x86_64.zip"
sudo unzip -o /tmp/protoc.zip bin/protoc -d /usr/local
protoc --version
