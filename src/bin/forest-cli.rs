// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() -> anyhow::Result<()> {
    forest_filecoin::forest_main(std::env::args_os())
}
