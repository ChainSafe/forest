// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};

use clap::Subcommand;
use futures::{StreamExt, TryStreamExt};
use fvm_ipld_blockstore::Blockstore;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufReader},
};

use crate::utils::db::{
    car_stream::CarStream,
    car_util::{dedup_block_stream, merge_car_streams},
};
use crate::{db::car::ForestCar, rpc_client::ApiInfo};

#[derive(Debug, Subcommand)]
pub enum ApiCommands {
    /// Compare
    Compare {
        /// Compare endpoints for completeness and compatibility
        apis: Vec<ApiInfo>,
    },
}

impl ApiCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Compare { apis } => (),
        }
        Ok(())
    }
}
