#!/usr/bin/env bash
# Regenerates the `.hex` bytecode beside every `.sol` fixture here. The tests consume only the
# `.hex`, which is checked in so neither CI nor a developer needs a Solidity toolchain.
#
# The compiler is pinned and each source pins the same version in its `pragma`: bytecode has to
# be reproducible, and a different compiler shifts the gas profile these tests measure.
set -euo pipefail

SOLC_VERSION=0.8.30
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"

for sol in "${DIR}"/*/*.sol; do
  contract_dir="$(dirname "${sol}")"
  name="$(basename "${sol}" .sol)"
  echo "compiling ${name} with solc ${SOLC_VERSION}"
  docker run --rm --volume "${contract_dir}:/src:ro" "ethereum/solc:${SOLC_VERSION}" \
    --bin "/src/${name}.sol" |
    awk '/^Binary:/ { getline; if ($0 != "") print }' > "${contract_dir}/${name}.hex"
done
