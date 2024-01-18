#!/usr/bin/env bash

set -euxo pipefail

FOREST_TOOL_PATH="forest-tool"
PORTS=(8080 8081)

# Function to get the number of files in the present working directory
num_files_here() {
    find . -type f | wc --lines
}

# Function to stop services on specified ports
stop_services() {
    for port in "${PORTS[@]}"; do
        fuser -k "$port/tcp" || true
    done
}

TEMP_DIR=$(mktemp --directory)
pushd "$TEMP_DIR"
    # Fetch latest calibnet snapshot
    "$FOREST_TOOL_PATH" snapshot fetch --chain calibnet
    test "$(num_files_here)" -eq 1
    snapshot=$(find . -type f | head -1)

    # Start Node-Less RPC servers on ports 8080 and 8081
    for port in "${PORTS[@]}"; do
        "$FOREST_TOOL_PATH" api serve "$snapshot" --chain calibnet --port "$port" &
    done

    # Check if services on ports 8080 and 8081 have started
    while ! (nc -z localhost 8080 && nc -z localhost 8081); do
        sleep 30
    done

    # Compare
    result="$($FOREST_TOOL_PATH api compare "$snapshot" --forest /ip4/127.0.0.1/tcp/8080/http --lotus /ip4/127.0.0.1/tcp/8081/http)"

    # Check the result
    if echo "$result" | grep -E -v "\| *(Valid|Timeout) *\| *(Valid|Timeout) *\|"; then
        stop_services
        exit 1
    fi
popd

# Stop services on ports 8080 and 8081
stop_services

# Cleanup temporary directory
rm -rf "$TEMP_DIR"
