// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() -> anyhow::Result<()> {
    forest::forest_wallet_main(std::env::args_os())
}
