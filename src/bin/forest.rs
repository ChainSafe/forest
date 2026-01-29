// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() -> anyhow::Result<()> {
    forest::forestd_main(std::env::args_os())
}
