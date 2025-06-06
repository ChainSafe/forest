#!/bin/bash
# This script compiles all the Solidity files in the current directory and
# generates the corresponding files with the compiled bytecode in hexadecimal
# format.
#
# Requires solc version 0.8.30 for reproducible builds.

set -euo pipefail

REQUIRED_SOLC_VERSION="0.8.30"

# Check if solc exists
if ! command -v solc &>/dev/null; then
    echo "ERROR: solc not found. Install solc version $REQUIRED_SOLC_VERSION"
    exit 1
fi

# Extract solc version number
solc_version=$(solc --version | awk '/Version:/ {print $2}' | cut -d'+' -f1)

if [[ "$solc_version" != "$REQUIRED_SOLC_VERSION" ]]; then
    echo "ERROR: Required solc version $REQUIRED_SOLC_VERSION, found $solc_version"
    echo "Install correct version: solc-select install $REQUIRED_SOLC_VERSION && solc-select use $REQUIRED_SOLC_VERSION"
    exit 1
fi

echo "Using solc version: $solc_version"

find . -mindepth 2 -type f -name "*.sol" -print0 | while IFS= read -r -d '' file; do
    echo "Compiling: $file"

    # Extract directory and base name
    dir=$(dirname "$file")
    base_name=$(basename "$file" .sol)
    hex_file="$dir/$base_name.hex"

    # Compile and capture output
    if ! solc_output=$(solc --optimize --bin "$file" 2>&1); then
        echo "ERROR: Compilation failed for $file"
        echo "solc output: $solc_output"
        exit 1
    fi

    # Extract bytecode (last line of successful compilation)
    bytecode=$(echo "$solc_output" | tail -n 1 | tr -d '\n')

    if [[ -z "$bytecode" ]]; then
        echo "ERROR: Generated bytecode is empty for $file"
        echo "solc output: $solc_output"
        exit 1
    fi

    # Write to hex file
    echo -n "$bytecode" >"$hex_file"

    echo "Generated: $dir/$base_name.hex"
done
