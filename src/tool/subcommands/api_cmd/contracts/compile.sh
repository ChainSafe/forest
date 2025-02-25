#!/bin/bash
# This script compiles all the Solidity files in the current directory and
# generates the corresponding files with the compiled bytecode in hexadecimal
# format.

set -euo pipefail

find . -maxdepth 1 -type f -name "*.sol" -print0 | while IFS= read -r -d '' file; do
    base_name="${file%.sol}"
    solc --bin "$file" | tail -n 1 | tr -d '\n' > "$base_name.hex"
done
