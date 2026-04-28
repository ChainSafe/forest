#!/usr/bin/env bash
# Orchestrates the calibnet daemon (snapshot import, node spawn, sync wait)
# via harness.sh, then runs the `forest-wallet-tests delegated` Rust binary
# which performs the actual delegated wallet test logic.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

forest_wallet_init "$@"

forest-wallet-tests delegated
