#!/bin/bash
set -euo pipefail
# Install mise securely by verifying the signature. It is meant to be run exclusively for installing mise in a Docker image.

export GPG_KEY=24853EC9F655CE80B48E6C3A8B81C9D17413A06D
export MISE_VERSION=v2025.12.0
export MISE_INSTALL_PATH=/bin/mise

gpg --keyserver hkps://keys.openpgp.org --recv-keys ${GPG_KEY}
curl --fail https://mise.jdx.dev/install.sh.sig | gpg --decrypt > install-mise.sh
# ensure the above is signed with the mise release key
sh ./install-mise.sh
rm install-mise.sh
