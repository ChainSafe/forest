#!/usr/bin/env bash
# Drives the calibnet wallet integration tests in `tests/calibnet_wallet.rs`.
# `harness.sh::forest_wallet_init` brings up the daemon, imports the
# preloaded wallet into both backends, and exports `FULLNODE_API_INFO`.

set -euxo pipefail

source "$(dirname "$0")/harness.sh"

forest_wallet_init "$@"

cargo test --profile quick-test --test calibnet_wallet -- --ignored --nocapture
