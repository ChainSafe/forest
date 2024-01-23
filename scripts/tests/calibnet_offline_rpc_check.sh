#!/usr/bin/env bash
# This script is used to test the offline RPC API server against itself.
# It's run in CI, and uses forest-tool api compare subcommand to test RPC endpoints.

set -euxo pipefail

FOREST_TOOL_PATH="forest-tool"
PORTS=(8080 8081)

# Function to stop services on specified ports
stop_services() {
    for port in "${PORTS[@]}"; do
        fuser -k "$port/tcp" || true
    done
}

# Fetch latest calibnet snapshot
snapshot="$("$FOREST_TOOL_PATH" snapshot fetch --chain calibnet)"

# Start Offline RPC servers on ports
for i in "${!PORTS[@]}"; do
  port=${PORTS[$i]}
  data_dir="offline-rpc-db-$((i + 1))"
  "$FOREST_TOOL_PATH" api serve "$snapshot" --chain calibnet --port "$port" --data-dir "$data_dir" &
done

# Check if services on ports have started
for port in "${PORTS[@]}"; do
    until nc -z localhost "$port"; do
        sleep 30
    done
done

# Compare
$FOREST_TOOL_PATH api compare "$snapshot" --forest /ip4/127.0.0.1/tcp/8080/http --lotus /ip4/127.0.0.1/tcp/8081/http
exit_code=$?

# Check the result
if [ $exit_code -ne 0 ]; then
    stop_services
    exit 1
fi

# Stop services on ports
stop_services

# Cleanup temporary directory
rm -rf "$TEMP_DIR"
