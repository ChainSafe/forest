// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    forest::forest_main(std::env::args_os()).await
}
